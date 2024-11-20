use crate::{
  amdahline::Amdahline,
  config::{LocationID, PortID, ADDRESSES},
};
use serde::{Deserialize, Serialize};
use std::{
  collections::HashMap,
  io::{Read, Write},
  ops::Deref,
  panic,
  path::PathBuf,
  sync::{atomic::AtomicBool, Arc},
};
use strum::IntoEnumIterator;
use tokio::{
  io::{AsyncReadExt, AsyncWriteExt, BufReader, BufWriter},
  net::{TcpListener, TcpStream},
  sync::{Notify, RwLock},
  task::JoinHandle,
};

const BUFFER_SIZE: usize = 32 * 1024;

#[derive(Debug)]
/// StepArgument is an enum that represents the argument of a step command.
pub enum StepArgument {
  /// The argument is the value from the given port
  Port(PortID),
  /// The argument is a string
  String(String),
}

impl From<PortID> for StepArgument {
  fn from(port: PortID) -> Self {
    StepArgument::Port(port)
  }
}

impl From<String> for StepArgument {
  fn from(s: String) -> Self {
    StepArgument::String(s)
  }
}

impl From<&str> for StepArgument {
  fn from(s: &str) -> Self {
    StepArgument::String(s.to_string())
  }
}

#[derive(Debug)]
/// StepOutput is an enum that represents the output of a step.
pub enum StepOutput {
  /// The step is expected to write the output to a file at the given path
  File(String),
  /// The step is expected to write the output to the standard output
  Stdout,
  None,
}

#[derive(Eq, PartialEq, Hash, Serialize, Deserialize, Debug, Clone)]
pub enum PortData {
  File(String),
  String(String),
  Int(i32),
  Bool(bool),
  Empty,
}

impl PortData {
  pub fn is_empty(&self) -> bool {
    match self {
      PortData::Empty => true,
      _ => false,
    }
  }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Message {
  pub sender: LocationID,
  pub port: PortID,
  pub message_value: PortData,
  pub data_size: usize,
}

pub struct Port {
  pub port_ready: Notify,
  pub value: RwLock<PortData>,
}

impl Port {
  pub async fn set(&self, value: PortData) {
    *self.value.write().await = value;
  }
}

pub struct Communicator {
  ports: Arc<HashMap<PortID, Port>>,
  location: LocationID,
  workdir: PathBuf,
  incoming_messages: Arc<RwLock<HashMap<(LocationID, PortID), (Message, BufReader<TcpStream>)>>>,
  amdahline: Arc<Amdahline>,
}

impl Communicator {
  pub async fn new(
    location: LocationID,
    workdir: PathBuf,
    amdahline: Arc<Amdahline>,
  ) -> Communicator {
    let mut workdir = workdir;
    let mut data = HashMap::new();

    // initialize data ports
    for port in PortID::iter() {
      data.insert(
        port,
        Port {
          port_ready: Notify::new(),
          value: RwLock::new(PortData::Empty),
        },
      );
    }
    // make workdir absolute
    if workdir.is_relative() {
      workdir = std::env::current_dir()
        .expect("failed to get current dir")
        .join(workdir);
    }

    // create the workdir if it does not exist
    let err = std::fs::create_dir_all(&workdir);

    match err {
      Ok(_) => {}
      Err(e) => {
        println!(
          "{} PANIC: Failed to create workdir at {:?}, error: {:?}",
          debug_prelude(&location, None),
          workdir,
          e
        );
        panic!("Failed to create workdir at {:?}, error: {:?}", workdir, e);
      }
    }

    let comm = Communicator {
      ports: Arc::new(data),
      location,
      workdir,
      incoming_messages: Arc::new(RwLock::new(HashMap::new())),
      amdahline,
    };

    comm.accept_connections().await;

    comm
  }

  async fn accept_connections(&self) {
    let listener = |self_location: LocationID,
                    address: String,
                    incoming_messages: Arc<
      RwLock<HashMap<(LocationID, PortID), (Message, BufReader<TcpStream>)>>,
    >,
                    amdahline: Arc<Amdahline>| async move {
      let listener = TcpListener::bind(&address)
        .await
        .expect("failed to bind listener");

      println!(
        "{} Listening on {:?}",
        debug_prelude(&self_location, None),
        address
      );

      let _ = amdahline.begin_task(
        format!("{:?}", self_location),
        "accept_connections".to_string(),
      );

      // accept connections and add them to the incoming connections
      loop {
        let (mut stream, _) = listener
          .accept()
          .await
          .expect("failed to accept connection");

        // the sender will first send the message struct, then the data
        let mut buffer = vec![0; 1024];
        stream
          .read_exact(&mut buffer)
          .await
          .expect("failed to read message");

        let message: Message =
          bincode::deserialize(&buffer).expect("failed to deserialize message");

        let reader = BufReader::new(stream);

        // save the message
        incoming_messages
          .write()
          .await
          .insert((message.sender, message.port), (message, reader));
      }
    };

    tokio::spawn(listener(
      self.location,
      ADDRESSES
        .get(&self.location)
        .expect("failed to get address")
        .clone(),
      self.incoming_messages.clone(),
      self.amdahline.clone(),
    ));
  }

  pub fn close_connections(&self) {
  }

  pub async fn init_port(&self, port: PortID, value: PortData) {
    let data = self.ports.get(&port).expect("port not found");
    data.set(value).await;
    data.port_ready.notify_waiters();
  }

  pub async fn read_port(&self, port: PortID) -> PortData {
    let data = self
      .ports
      .get(&port)
      .expect("port not found")
      .value
      .read()
      .await;
    data.clone()
  }

  async fn wait_for_port(port: &Port) {
    if port.value.read().await.is_empty() {
      port.port_ready.notified().await;
    }

    if port.value.read().await.is_empty() {
      panic!("Port is empty, this should NOT happen");
    }
  }
}

/**
 * Send data of a specified port to a remote location
 * after the future is awaited, the port has been read and can be modified, but the send has not completed
 * to wait for the send to complete, await the returned handle
 */
pub async fn send(
  comm: Arc<Communicator>,
  port_id: PortID,
  destination: LocationID,
) -> JoinHandle<()> {
  //============================ Copy Data ============================
  // copies the data to a local buffer to be sent
  // after the data is copied, the port can be modified and the send will not be affected
  //===================================================================
  let ports = comm.ports.clone();
  let port = ports.get(&port_id).expect("port not found");
  let amdahline = comm.amdahline.clone();

  Communicator::wait_for_port(port).await;

  let data = port.value.read().await;
  let data = data.clone();

  //============================ Send Data ============================
  tokio::spawn(async move {
    let mut stream = connect_to(&comm.location, &destination, &port_id).await;

    let t = amdahline.begin_task(
      format!("{:?}", comm.location),
      format!("send {:?} -> {:?}", port_id, destination),
    );
    println!(
      "{} Sending data {:?} -> {:?}",
      debug_prelude(&comm.location, None),
      port_id,
      destination
    );

    //============================= Preparing data =============================
    match data {
      PortData::File(path) => {
        // open file
        let mut file = std::fs::File::open(&path).expect("failed to open file");

        let name = PathBuf::from(&path)
          .file_name()
          .expect("failed to get file name")
          .to_str()
          .expect("failed to convert to string")
          .to_string();

        let message = Message {
          sender: comm.location,
          port: port_id,
          message_value: PortData::File(name),
          data_size: file.metadata().expect("failed to get metadata").len() as usize,
        };

        let mut message = bincode::serialize(&message).expect("failed to serialize message");
        assert!(
          message.len() <= 1024,
          "{} PANIC: message too large: {:?}",
          debug_prelude(&comm.location, None),
          message.len()
        );
        message.resize(1024, 0);

        // read file and send BUFFER_SIZE Bytes at a time
        let mut buffer = [0; BUFFER_SIZE];

        // send message
        stream
          .write_all(&message)
          .await
          .expect("failed to write message");

        // send file
        while file.read(&mut buffer).expect("failed to read file") > 0 {
          stream
            .write_all(&buffer)
            .await
            .expect("failed to write file");
        }

        stream.flush().await.expect("failed to flush stream");
        stream.shutdown().await.expect("failed to shutdown stream");
      }
      PortData::Empty => {
        println!("PANIC: empty data");
        panic!("empty data");
      }
      data => {
        let message = Message {
          sender: comm.location,
          port: port_id,
          message_value: data,
          data_size: 0,
        };

        let message = bincode::serialize(&message).expect("failed to serialize message");

        stream
          .write_all(&message)
          .await
          .expect("failed to write message");
        stream.flush().await.expect("failed to flush stream");
        stream.shutdown().await.expect("failed to shutdown stream");
      }
    };

    println!(
      "{} Sent data {:?} -> {:?}",
      debug_prelude(&comm.location, None),
      port_id,
      destination
    );

    amdahline.end_task(format!("{:?}", comm.location), t);
  })
}

async fn connect_to(
  source: &LocationID,
  destination: &LocationID,
  port_id: &PortID,
) -> BufWriter<TcpStream> {
  let address = ADDRESSES
    .get(&destination)
    .expect("failed to get address")
    .clone();

  let mut stream;

  while {
    stream = TcpStream::connect(&address).await;
    stream.is_err()
  } {
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
  }

  let stream = stream.unwrap();

  let writer = BufWriter::new(stream);

  writer
}

/**
 * Receive data from a remote and store it in the specified port
 * after the future is awaited, the receive has started but not completed
 * to wait for the receive to complete, await the returned handle
 */
pub async fn receive(comm: Arc<Communicator>, port: PortID, sender: LocationID) -> JoinHandle<()> {
  comm
    .ports
    .get(&port)
    .expect("port not found")
    .set(PortData::Empty)
    .await;
  let amdahline = comm.amdahline.clone();

  tokio::spawn(async move {
    //===================== Connection =====================
    while comm
      .incoming_messages
      .read()
      .await
      .get(&(sender, port))
      .is_none()
    {
      tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }

    let t = amdahline.begin_task(
      format!("{:?}", comm.location),
      format!("receive {:?} <- {:?}", port, sender),
    );

    println!(
      "{} Receiving data {:?} <- {:?}",
      debug_prelude(&comm.location, None),
      port,
      sender
    );

    let (message, mut stream) = comm
      .incoming_messages
      .write()
      .await
      .remove(&(sender, port))
      .expect("failed to remove message");

    //===================== Data Handling =====================
    let port_data = comm.ports.get(&port).expect("port not found");

    match message.message_value {
      PortData::File(name) => {
        let path = comm.workdir.join(format!("receive_{:?}", comm.location));
        std::fs::create_dir_all(&path).expect("failed to create receive dir");

        let path = path.join(&name);
        let file = std::fs::File::create(&path).expect("failed to create file");

        let mut file = std::io::BufWriter::new(file);

        let mut buffer = [0; BUFFER_SIZE];

        while let Ok(size) = stream.read(&mut buffer).await {
          if size == 0 {
            break;
          }
          file
            .write_all(&buffer[..size])
            .expect("failed to write to file");
        }

        file.flush().expect("failed to flush file");

        port_data
          .set(PortData::File(
            path
              .to_str()
              .expect("failed to convert to string")
              .to_string(),
          ))
          .await;
      }
      PortData::Empty => {
        panic!("empty data");
      }
      data => {
        port_data.set(data).await;
      }
    }

    println!(
      "{} Received data {:?} <- {:?}",
      debug_prelude(&comm.location, None),
      port,
      sender
    );

    port_data.port_ready.notify_waiters();

    amdahline.end_task(format!("{:?}", comm.location), t);
  })
}

/**
 * Execute a command and store the output in the specified port
 * after the future is awaited, the command has gathered all the necessary data and is ready to be executed
 * to wait for the command to complete, await the returned handle
 */
pub async fn exec(
  comm: Arc<Communicator>,
  step_name: String,
  step_display_name: String,
  input_ports: Vec<PortID>,
  output_port: Option<PortID>,
  output_type: StepOutput,
  cmd: String,
  args: Vec<StepArgument>,
) {
  let amdahline = comm.amdahline.clone();

  let mut step_workdir = comm.workdir.join(format!("step_{}", step_name));

  let step_workdir_str = step_workdir.to_str().expect("failed to convert to string");
  let step_workdir_str = format!("failed to convert to string: {:?}", step_workdir_str);
  std::fs::create_dir_all(&step_workdir).expect(&step_workdir_str);

  step_workdir = comm.workdir.join(format!("step_{}", step_name));
  step_workdir = step_workdir
    .canonicalize()
    .expect(format!("failed to canonicalize {:?}", step_workdir).as_str());

  // loop over the ports
  for input_port in input_ports {
    let port = comm.ports.get(&input_port).expect("port not found");

    Communicator::wait_for_port(port).await;

    let data = port.value.read().await;
    let data = data.deref();

    match data {
      PortData::File(path) => {
        // link the file to the step workdir
        let file_path = PathBuf::from(path);
        let file_name = file_path
          .file_name()
          .expect("failed to get file name")
          .to_str()
          .expect("failed to convert to string")
          .to_string();

        let new_path = step_workdir.join(&file_name);

        // create symlink
        #[cfg(unix)]
        {
          std::os::unix::fs::symlink(&file_path, &new_path).expect("failed to create symlink");
        }
      }
      PortData::Empty => {
        panic!("empty data");
      }
      _ => {}
    }
  }

  //======================== Build arguments ========================
  let mut arguments: Vec<String> = vec![];

  for arg in args {
    match arg {
      StepArgument::String(value) => {
        arguments.push(value);
      }
      StepArgument::Port(port_id) => {
        let port = comm.ports.get(&port_id).expect("port not found");

        Communicator::wait_for_port(port).await;

        let data = port.value.read().await;
        let data = data.deref();

        match data {
          PortData::File(path) => {
            // if the argument is a file, the file should be already linked to the step workdir
            let filename = PathBuf::from(path)
              .file_name()
              .expect("failed to get file name")
              .to_str()
              .expect("failed to convert to string")
              .to_string();
            arguments.push(filename);
          }
          PortData::String(value) => {
            arguments.push(value.clone());
          }
          PortData::Int(value) => {
            arguments.push(value.to_string());
          }
          PortData::Bool(value) => {
            arguments.push(value.to_string());
          }
          PortData::Empty => {
            panic!("empty data");
          }
        }
      }
    }
  }

  //======================== Execute Command ========================
  let t = amdahline.begin_task(
    format!("{:?}", comm.location),
    format!("exec {:?} ({})", step_name, step_display_name),
  );

  let mut cmd = cmd;

  #[cfg(windows)]
  {
    arguments.insert(0, cmd);
    arguments.insert(0, "-Command".to_string());

    cmd = format!("powershell");
  }

  let arguments = arguments.join(" ");

  println!(
    "{} Running command: '{} {}'",
    debug_prelude(&comm.location, Some(&step_name)),
    cmd,
    arguments
  );

  let start_time = std::time::Instant::now();

  let (output, status) = match output_type {
    StepOutput::Stdout => {
      let child = tokio::process::Command::new(&cmd)
        .args(arguments.split_whitespace())
        .current_dir(&step_workdir)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("failed to spawn command");

      println!(
        "{} Command spawned",
        debug_prelude(&comm.location, Some(&step_name))
      );

      let output = child
        .wait_with_output()
        .await
        .expect("failed to wait with output");
      let status = output.status;

      (Some(output), status)
    }
    _ => {
      let mut child = tokio::process::Command::new(&cmd)
        .args(arguments.split_whitespace())
        .current_dir(&step_workdir)
        .stdout(std::process::Stdio::null())
        .spawn()
        .expect("failed to spawn command");

      println!(
        "{} Command spawned",
        debug_prelude(&comm.location, Some(&step_name))
      );

      let status = child.wait().await.expect("failed to wait");

      (None, status)
    }
  };

  if !status.success() {
    panic!("Command failed with status: {}", status);
  }

  println!(
    "{} Completed step: {} in {}s",
    debug_prelude(&comm.location, Some(&step_name)),
    step_name,
    start_time.elapsed().as_secs()
  );

  if output_port.is_none() {
    return;
  }

  let port = comm
    .ports
    .get(&output_port.unwrap())
    .expect("port not found");

  match output_type {
    StepOutput::File(path_regex) => {
      let path_regex = path_regex.replace("/", "\\");

      let path_regex = step_workdir.join(path_regex);

      let path_regex = path_regex
        .to_str()
        .expect("failed to convert to string")
        .to_string();

      let res = glob::glob(path_regex.as_str()).expect("failed to glob");
      let res = res
        .collect::<Result<Vec<_>, _>>()
        .expect("failed to collect");

      if res.len() == 0 {
        let available_files = std::fs::read_dir(&step_workdir)
          .expect("failed to read dir")
          .map(|res| res.unwrap().path())
          .collect::<Vec<_>>();
        panic!(
          "No files found for regex: {}, available files: {:?}",
          path_regex, available_files
        );
      }

      if res.len() > 1 {
        panic!("Multiple files found for regex: {}", path_regex);
      }

      let path = res[0]
        .to_str()
        .expect("failed to convert to string")
        .to_string();

      port.set(PortData::File(path)).await;
      port.port_ready.notify_waiters();
    }
    StepOutput::Stdout => {
      let stdout = String::from_utf8(output.expect("failed to get output").stdout)
        .expect("failed to convert output to string");

      port.set(PortData::String(stdout)).await;
      port.port_ready.notify_waiters();
    }
    StepOutput::None => {
      port.set(PortData::Empty).await;
      port.port_ready.notify_waiters();
    }
  }

  amdahline.end_task(format!("{:?}", comm.location), t);
}

pub fn debug_prelude(location: &LocationID, steap_name: Option<&String>) -> String {
  // [HH:MM:SS] [location] [step_name] >>>
  let time = chrono::Local::now().format("%H:%M:%S").to_string();
  let location = format!("{:?}", location);
  let step_name = match steap_name {
    Some(step_name) => format!(" [{}]", step_name),
    None => "".to_string(),
  };

  format!("[{}] [{}]{} >>> ", time, location, step_name)
}
