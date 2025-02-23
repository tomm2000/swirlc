use crate::{
  amdahline::Amdahline,
  config::{Addresses, LocationID, PortID},
  orchestra::utils::{data_size, debug_prelude},
};
use serde::{Deserialize, Serialize};
use std::{
  collections::HashMap,
  io::Write,
  ops::Deref,
  panic,
  path::PathBuf,
  sync::Arc, thread,
};
use strum::IntoEnumIterator;
use tokio::{
  io::{AsyncReadExt, AsyncWriteExt, BufReader, BufWriter},
  net::{TcpListener, TcpStream},
  sync::{Notify, RwLock},
  task::JoinHandle,
};

const BUFFER_SIZE: usize = 8 * 1024;

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
  addresses: Addresses,
}

impl Communicator {
  pub async fn new(
    location: LocationID,
    workdir: PathBuf,
    amdahline: Arc<Amdahline>,
    addresses: Addresses,
  ) -> Arc<Communicator> {
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

    let comm = Arc::new(Communicator {
      ports: Arc::new(data),
      location,
      workdir,
      incoming_messages: Arc::new(RwLock::new(HashMap::new())),
      amdahline,
      addresses,
    });

    let comm_clone = comm.clone();

    comm_clone.accept_connections();

    comm
  }


  fn accept_connections(self: &Arc<Self>) {
    // let rt = tokio::runtime::Builder::new_multi_thread()
    //   .worker_threads(1)
    //   .enable_all()
    //   .build()
    //   .expect("failed to build runtime");

    let comm = self.clone();

    tokio::spawn(async move {
      let address = comm.addresses.get_address(comm.location);

      let tcp_listener = TcpListener::bind(&address)
        .await
        .expect(format!("{} failed to bind to address: {:?}", debug_prelude(&comm.location, None), address).as_str());
  
      println!(
        "{} Listening on {:?}",
        debug_prelude(&comm.location, None),
        address
      );
  
      let _ = comm.amdahline.begin_task(
        format!("{:?}", comm.location),
        "accept_connections".to_string(),
      );
  
      // accept connections and add them to the incoming connections
      loop {
        let (mut stream, _) = tcp_listener
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
        let mut inc_message_write = comm.incoming_messages.write().await;
  
        inc_message_write
          .insert((message.sender, message.port), (message, reader));
  
        drop(inc_message_write);
      }
    });
  }

  pub fn close_connections(&self) {}

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

  /**
   * Send data of a specified port to a remote location
   * after the future is awaited, the port has been read and can be modified, but the send has not completed
   * to wait for the send to complete, await the returned handle
   */
  pub async fn send(
    self: &Arc<Self>,
    port_id: PortID,
    destination: LocationID,
  ) -> JoinHandle<()> {
    //============================ Copy Data ============================
    // copies the data to a local buffer to be sent
    // after the data is copied, the port can be modified and the send will not be affected
    //===================================================================
    let comm = self.clone();
    let port = self.ports.get(&port_id).expect("port not found");

    Communicator::wait_for_port(port).await;

    let data_read = port.value.read().await;
    let data = data_read.clone();

    drop(data_read);

    //============================ Send Data ============================
    tokio::spawn(async move {
      let start_time = std::time::Instant::now();

      let mut stream = Communicator::connect_to(
        comm.amdahline.clone(), 
        &comm.location,
        &destination,
        &port_id,
        &comm.addresses
      ).await;

      let t1 = start_time.elapsed().as_millis();
      let start_time = std::time::Instant::now();

      let t = comm.amdahline.begin_task(
        format!("{:?}", comm.location),
        format!("send {:?} -> {:?}", port_id, destination),
      );

      println!(
        "{} Sending data {:?} -> {:?}",
        debug_prelude(&comm.location, None),
        port_id,
        destination
      );

      let t2 = start_time.elapsed().as_millis();

      //============================= Preparing data =============================
      match data {
        PortData::File(path) => {
          let start_time = std::time::Instant::now();

          let file = tokio::fs::File::options()
            .read(true)
            .open(&path)
            .await
            .expect(format!("failed to open file: {:?}", path).as_str());

          let file_size: usize = file.metadata().await.expect("failed to get metadata").len() as usize;
          let mut reader = BufReader::new(file);

          let name = PathBuf::from(&path)
            .file_name()
            .expect("failed to get file name")
            .to_str()
            .expect("failed to convert to string")
            .to_string();

          let t3 = start_time.elapsed().as_millis();
          let start_time = std::time::Instant::now();

          let message = Message {
            sender: comm.location,
            port: port_id,
            message_value: PortData::File(name),
            data_size: file_size,
          };

          let mut message = bincode::serialize(&message).expect("failed to serialize message");

          assert!(
            message.len() <= 1024,
            "{} PANIC: message too large: {:?}",
            debug_prelude(&comm.location, None),
            message.len()
          );
          message.resize(1024, 0);

          // send message
          stream
            .write_all(&message)
            .await
            .expect("failed to write message");

          let t4 = start_time.elapsed().as_millis();
          let start_time = std::time::Instant::now();

          let total_bytes = tokio::io::copy(&mut reader, &mut stream).await.expect("failed to copy file") as usize;

          reader.flush().await.expect("failed to flush reader");
          stream.shutdown().await.expect("failed to shutdown stream");

          println!(
            "{} Sent file {:?} -> {:?}, size: {}",
            debug_prelude(&comm.location, None),
            port_id,
            destination,
            data_size(total_bytes)
          );

          let t5 = start_time.elapsed().as_millis();
          println!("{} Time: t1: {}, t2: {}, t3: {}, t4: {}, t5: {}", debug_prelude(&comm.location, None), t1, t2, t3, t4, t5);
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

      comm.amdahline.end_task(format!("{:?}", comm.location), t);
    })
  }

  
  async fn connect_to(
    amdahline: Arc<Amdahline>,
    _source: &LocationID,
    destination: &LocationID,
    _port_id: &PortID,
    addresses: &Addresses,
  ) -> BufWriter<TcpStream> {
    let address = addresses.get_address(*destination);

    let mut stream;

    while {
      stream = TcpStream::connect(&address).await;
      stream.is_err()
    } {
      tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
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
  pub async fn receive(
    self: &Arc<Self>,
    port: PortID,
    sender: LocationID,
  ) -> JoinHandle<()> {
    self
      .ports
      .get(&port)
      .expect("port not found")
      .set(PortData::Empty)
      .await;

    let comm = self.clone();

    tokio::spawn(async move {
      //===================== Connection =====================
      while comm.incoming_messages
        .read()
        .await
        .get(&(sender, port))
        .is_none()
      {
        tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
      }

      let t = comm.amdahline.begin_task(
        format!("{:?}", comm.location),
        format!("receive {:?} <- {:?}", port, sender),
      );

      println!(
        "{} Receiving data {:?} <- {:?}",
        debug_prelude(&comm.location, None),
        port,
        sender
      );

      let mut inc_message_write = comm.incoming_messages.write().await;

      let (message, mut stream) = inc_message_write
        .remove(&(sender, port))
        .expect("failed to remove message");

      drop(inc_message_write);

      //===================== Data Handling =====================
      let port_data = comm.ports.get(&port).expect("port not found");

      match message.message_value {
        PortData::File(name) => {
          let path = comm.workdir.join(format!("receive_{:?}", comm.location));
          std::fs::create_dir_all(&path).expect("failed to create receive dir");

          let path = path.join(&name);
          let file = tokio::fs::File::create(&path).await.expect("failed to create file");
          let mut file = BufWriter::new(file);

          let total_bytes = tokio::io::copy(&mut stream, &mut file).await.expect("failed to copy file") as usize;

          println!(
            "{} Received file {:?} <- {:?}, size: {}",
            debug_prelude(&comm.location, None),
            port,
            sender,
            data_size(total_bytes)
          );

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

          println!(
            "{} Received data {:?} <- {:?}",
            debug_prelude(&comm.location, None),
            port,
            sender
          );
        }
      }

      port_data.port_ready.notify_waiters();

      comm.amdahline.end_task(format!("{:?}", comm.location), t);
    })
  }

  /**
   * Execute a command and store the output in the specified port
   * after the future is awaited, the command has gathered all the necessary data and is ready to be executed
   * to wait for the command to complete, await the returned handle
   */
  pub async fn exec(
    &self,
    step_name: String,
    step_display_name: String,
    input_ports: Vec<PortID>,
    output_port: Option<PortID>,
    output_type: StepOutput,
    cmd: String,
    args: Vec<StepArgument>,
  ) {
    let mut step_workdir = self.workdir.join(format!("step_{}", step_name));

    let step_workdir_str = step_workdir.to_str().expect("failed to convert to string");
    let step_workdir_str = format!("failed to convert to string: {:?}", step_workdir_str);
    std::fs::create_dir_all(&step_workdir).expect(&step_workdir_str);

    step_workdir = self.workdir.join(format!("step_{}", step_name));
    step_workdir = step_workdir
      .canonicalize()
      .expect(format!("failed to canonicalize {:?}", step_workdir).as_str());

    // loop over the ports
    for input_port in input_ports {
      let port = self.ports.get(&input_port).expect("port not found");

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
          let port = self.ports.get(&port_id).expect("port not found");

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
    let t = self.amdahline.begin_task(
      format!("{:?}", self.location),
      format!("exec {:?} ({})", step_name, step_display_name),
    );

    // let arguments = arguments.join(" ");

    println!(
      "{} Running command: '{} {}'",
      debug_prelude(&self.location, Some(&step_name)),
      cmd,
      arguments.join(" ")
    );

    let start_time = std::time::Instant::now();

    let (output, status) = match output_type {
      StepOutput::Stdout => {
        // let child = tokio::process::Command::new("sh")
          // .arg("-c")
          // .arg(format!("{} {}", cmd, arguments))
        let child = tokio::process::Command::new(&cmd)
          .args(arguments)
          .current_dir(&step_workdir)
          .stdout(std::process::Stdio::piped())
          .spawn()
          .expect(format!("failed to spawn command: {:?}", &cmd).as_str());

        println!(
          "{} Command spawned",
          debug_prelude(&self.location, Some(&step_name))
        );

        let output = child
          .wait_with_output()
          .await
          .expect("failed to wait with output");
        let status = output.status;

        (Some(output), status)
      }
      _ => {
        // let mut child = tokio::process::Command::new("sh")
        //   .arg("-c")
        //   .arg(format!("{} {}", cmd, arguments))
        let mut child = tokio::process::Command::new(&cmd)
          .args(arguments)
          .current_dir(&step_workdir)
          .stdout(std::process::Stdio::null())
          .spawn()
          .expect(format!("failed to spawn command: {:?}", &cmd).as_str());

        println!(
          "{} Command spawned",
          debug_prelude(&self.location, Some(&step_name))
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
      debug_prelude(&self.location, Some(&step_name)),
      step_name,
      start_time.elapsed().as_secs()
    );

    if output_port.is_some() {
      let port = self
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
    }

    self.amdahline.end_task(format!("{:?}", self.location), t);
  }
}
