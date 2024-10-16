use std::{collections::HashMap, io::{Read, Write}, ops::Deref, path::PathBuf, sync::{atomic::AtomicBool, Arc} };
use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::{TcpListener, TcpStream}, sync::{Notify, RwLock}, task::JoinHandle};
use crate::config::{LocationID, PortID, ADDRESSES};

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
  None
}

#[derive(Eq, PartialEq, Hash, Serialize, Deserialize, Debug, Clone)]
pub enum DataType {
  File(String),
  String(String),
  Int(i32),
  Bool(bool),
  Empty
}

impl DataType {
  pub fn is_empty(&self) -> bool {
    match self {
      DataType::Empty => true,
      _ => false
    }
  }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum MessageData {
  File(Vec<u8>),
  None
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Message {
  pub message_value: DataType,
  pub message_data: MessageData
}

pub struct Port {
  pub port_ready: Notify,
  pub value: RwLock<DataType>  
}

impl Port {
  pub async fn set(&self, value: DataType) {
    *self.value.write().await = value;
  }
}

pub struct Communicator {
  ports: Arc<HashMap<PortID, Port>>,
  location: LocationID,
  workdir: PathBuf,
  incoming_conn: Arc<RwLock<HashMap<LocationID, TcpStream>>>,
  closing: Arc<AtomicBool>
}

impl Communicator {
  pub fn new(location: LocationID, workdir: PathBuf) -> Communicator {
    let mut data = HashMap::new();

    // initialize data ports
    for port in PortID::iter() {
      data.insert(port, Port {
        port_ready: Notify::new(),
        value: RwLock::new(DataType::Empty)
      });
    }

    // create the workdir if it does not exist
    let err = std::fs::create_dir_all(&workdir);

    match err {
      Ok(_) => {},
      Err(e) => {
        panic!("[{:?}] > Failed to create workdir: {}", location, e);
      }
    }

    let comm = Communicator {
      ports: Arc::new(data),
      location,
      workdir,
      incoming_conn: Arc::new(RwLock::new(HashMap::new())),
      closing: Arc::new(AtomicBool::new(false))
    };

    comm.accept_connections();

    comm
  }

  fn accept_connections(&self) -> JoinHandle<()> {
    let address = ADDRESSES.get(&self.location).expect("location not found").clone();
    let self_location = self.location.clone();
    let incoming_conn = self.incoming_conn.clone();
    let closing = self.closing.clone();

    tokio::spawn(async move {
      //===================== Connection =====================
      let listener = TcpListener::bind(address).await.expect("failed to bind");

      // accept connections and add them to the incoming connections
      loop {
        let (mut stream, _) = listener.accept().await.expect("failed to accept");

        // first 1024 bytes are the location id
        let mut buffer = [0; 1024];
        stream.read_exact(&mut buffer).await.expect("failed to read");

        let incoming_location: LocationID = bincode::deserialize(&buffer).expect("failed to deserialize");

        println!("[{:?}] > Connection from {:?}", self_location, incoming_location);

        incoming_conn.write().await.insert(incoming_location, stream);

        if closing.load(std::sync::atomic::Ordering::Relaxed) {
          println!("[{:?}] > Closing incoming connections", self_location);

          for k in incoming_conn.write().await.keys() {
            let mut stream = incoming_conn.write().await.remove(k).expect("failed to get stream");
            stream.shutdown().await.expect("failed to shutdown");
          }

          break;
        }
      }
    })
  }

  pub fn close_connections(&self) {
    self.closing.store(true, std::sync::atomic::Ordering::Relaxed);
  }

  pub async fn init_port(&self, port: PortID, value: DataType) {
    let data = self.ports.get(&port).expect("port not found");
    data.set(value).await;
    data.port_ready.notify_waiters();

    println!("[{:?}] > Port {:?} initialized", self.location, port);
  }

  pub async fn read_port(&self, port: PortID) -> DataType {
    let data = self.ports.get(&port).expect("port not found").value.read().await;
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
pub async fn send(comm: Arc<Communicator>, port: PortID, location: LocationID) -> JoinHandle<()> {
  let remote_address = ADDRESSES.get(&location).expect("location not found").clone();

  //============================ Copy Data ============================
  // copies the data to a local buffer to be sent
  // after the data is copied, the port can be modified and the send will not be affected
  //===================================================================
  let ports = comm.ports.clone();
  let port = ports.get(&port).expect("port not found");

  Communicator::wait_for_port(port).await;

  let data = port.value.read().await;
  let data = data.clone();
  
  //============================ Send Data ============================
  tokio::spawn(async move {
    //============================= Connection =============================
    let mut stream;

    while {
      stream  = TcpStream::connect(&remote_address).await;
      stream.is_err()
    } {
      println!("[{:?}] > Awaiting connection to {}", comm.location, remote_address);
      tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }

    let mut stream = stream.expect("failed to connect");

    println!("[{:?}] > Connected (send) to {}", comm.location, remote_address);
    
    // send the location id to the remote, 1024 bytes
    let mut buffer = [0; 1024];
    bincode::serialize_into(&mut buffer[..], &comm.location).expect("failed to serialize");

    stream.write_all(&buffer).await.expect("failed to write");

    //============================= Communication =============================
    match data {
      DataType::File(path) => {
        // open file
        let mut file = std::fs::File::open(&path).expect("failed to open file");
        let mut buffer: Vec<u8> = vec![];
        file.read_to_end(&mut buffer).expect("failed to read file");

        let name = PathBuf::from(&path).file_name().expect("failed to get file name").to_str().expect("failed to convert to string").to_string();

        let message = Message {
          message_value: DataType::File(name),
          message_data: MessageData::File(buffer)
        };

        let message = bincode::serialize(&message).expect("failed to serialize");

        stream.write_all(&message).await.expect("failed to write");
      },
      DataType::String(data) => {
        let message = Message {
          message_value: DataType::String(data.clone()),
          message_data: MessageData::None
        };

        let message = bincode::serialize(&message).expect("failed to serialize");

        stream.write_all(&message).await.expect("failed to write");
      },
      DataType::Int(data) => {
        let message = Message {
          message_value: DataType::Int(data.clone()),
          message_data: MessageData::None
        };

        let message = bincode::serialize(&message).expect("failed to serialize");

        stream.write_all(&message).await.expect("failed to write");
      },
      DataType::Bool(data) => {
        let message = Message {
          message_value: DataType::Bool(data.clone()),
          message_data: MessageData::None
        };

        let message = bincode::serialize(&message).expect("failed to serialize");

        stream.write_all(&message).await.expect("failed to write");
      },
      DataType::Empty => { panic!("empty data"); }
    }
    
    // close connection
    stream.shutdown().await.expect("failed to shutdown");
  })
}

/**
 * Receive data from a remote and store it in the specified port
 * after the future is awaited, the receive has started but not completed
 * to wait for the receive to complete, await the returned handle
 */
pub async fn receive(comm: Arc<Communicator>, port: PortID, sender: LocationID) -> JoinHandle<()> {
    comm.ports.get(&port).expect("port not found").set(DataType::Empty).await;
    
    tokio::spawn(async move {
      //===================== Connection =====================
      while comm.incoming_conn.read().await.get(&sender).is_none() {
        println!("[{:?}] > Awaiting connection from {:?}", comm.location, sender);
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
      }

      let mut stream = comm.incoming_conn.write().await.remove(&sender).expect("failed to get stream");

      //===================== Communication =====================
      let mut buffer: Vec<u8> = vec![];
      stream.read_to_end(&mut buffer).await.expect("failed to read");
    
      let message: Message = bincode::deserialize(&buffer).expect("failed to deserialize");

      //===================== Data Handling =====================
      let port = comm.ports.get(&port).expect("port not found");

      match message.message_value {
        DataType::File(name) => {
          if let MessageData::File(data) = message.message_data {
            let path = comm.workdir.join("receive");
            std::fs::create_dir_all(&path).expect("failed to create receive dir");

            let path = path.join(&name);
            println!("[{:?}] > Received file: {:?}, {} bytes", comm.location, path, data.len());
            let file = std::fs::File::create(&path).expect("failed to create file");

            let mut file = std::io::BufWriter::new(file);
            file.write_all(&data).expect("failed to write file");
            port.set(DataType::File(path.to_str().expect("failed to convert to string").to_string())).await;
          } else {
            panic!("[{:?}] > Expected file data", comm.location);
          }
        },
        DataType::String(value) => {
          println!("[{:?}] > Received string: {:?}", comm.location, value);
          port.set(DataType::String(value)).await;
        },
        DataType::Int(value) => {
          println!("[{:?}] > Received int: {:?}", comm.location, value);
          port.set(DataType::Int(value)).await;
        },
        DataType::Bool(value) => {
          println!("[{:?}] > Received bool: {:?}", comm.location, value);
          port.set(DataType::Bool(value)).await;
        },
        DataType::Empty => { panic!("[{:?}] > empty data", comm.location); }
      }

      port.port_ready.notify_waiters();
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
  output_port: Option<PortID>,
  output_type: StepOutput,
  cmd: String,
  args: Vec<StepArgument>
) {
  println!("[{:?}] > Starting step: {}", comm.location, step_display_name);

  let step_workdir = comm.workdir.join(format!("step_{}", step_name));
  std::fs::create_dir_all(&step_workdir).expect("failed to create step workdir");

  //======================== Builds the arguments ========================
  let mut arguments: Vec<String> = vec![];

  for arg in args {
    match arg {
      StepArgument::String(value) => {
        arguments.push(value);
      },
      StepArgument::Port(port_id) => {
        let port = comm.ports.get(&port_id).expect("port not found");

        Communicator::wait_for_port(port).await;

        let data = port.value.read().await;
        let data = data.deref();

        match data {
          DataType::File(path) => {
            // if on unix, create a symbolic link
            #[cfg(unix)] {
              let file_path = PathBuf::from(path);
              let link_path = step_workdir.join(file_path.file_name().expect("failed to get file name"));

              println!("[{:?}] > Creating symbolic link: {:?} -> {:?}", self.self_location, file_path, link_path);

              std::os::unix::fs::symlink(file_path, link_path).expect("failed to create symbolic link");
              arguments.push(link_path.to_str().expect("failed to convert to string").to_string());
            }
            // if on windows, point to the file relative to the workdir
            #[cfg(windows)] {
              let file_path = PathBuf::from(path);
              let relative_path = pathdiff::diff_paths(&file_path, &step_workdir).expect("failed to get relative path");

              println!("[{:?}] > Using relative path: {:?}", comm.location, relative_path);

              arguments.push(relative_path.to_str().expect("failed to convert to string").to_string());
            }
          },
          DataType::String(value) => {
            arguments.push(value.clone());
          },
          DataType::Int(value) => {
            arguments.push(value.to_string());
          },
          DataType::Bool(value) => {
            arguments.push(value.to_string());
          },
          DataType::Empty => { panic!("empty data"); }
        }
      }
    }
  }
  
  //======================== Execute Command ========================
  let mut cmd = cmd;

  #[cfg(windows)] {
    arguments.insert(0, cmd);
    arguments.insert(0, "-Command".to_string());

    cmd = format!("powershell");
  }

  let arguments = arguments.join(" ");

  println!("[{:?}] > Running command: '{} {}'", comm.location, cmd, arguments);

  let output = std::process::Command::new(cmd)
    .args(arguments.split_whitespace())
    .current_dir(&step_workdir)
    .output()
    .expect("failed to execute process");

  if output_port.is_none() { return; }

  let port = comm.ports.get(&output_port.unwrap()).expect("port not found");

  match output_type {
    StepOutput::File(path_regex) => {
      let res = glob::glob(&format!("{}/{}", step_workdir.to_str().expect("failed to convert to string"), path_regex)).expect("failed to glob");
      let res = res.collect::<Result<Vec<_>, _>>().expect("failed to collect");

      if res.len() == 0 { panic!("No files found"); }
      if res.len() > 1 { panic!("Multiple files found"); }
      
      let path = res[0].to_str().expect("failed to convert to string").to_string();

      port.set(DataType::File(path)).await;
      port.port_ready.notify_waiters();
    },
    StepOutput::Stdout => {
      let stdout = String::from_utf8(output.stdout).expect("failed to convert output to string");
      
      port.set(DataType::String(stdout)).await;
      port.port_ready.notify_waiters();
    },
    StepOutput::None => {
      port.set(DataType::Empty).await;
      port.port_ready.notify_waiters();
    }
  }
}