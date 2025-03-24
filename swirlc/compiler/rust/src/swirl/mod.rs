pub mod send;
pub mod receive;
pub mod broadcast;
pub mod exec;
pub mod config;

use std::{collections::HashMap, path::PathBuf, sync::Arc};
use config::PORTS;
use serde::{Deserialize, Serialize};
use tokio::sync::{Notify, RwLock};

use crate::{amdahline::Amdahline, orchestra::{LocationInfo, Orchestra}};

// TODO: port id should be an enum
pub type PortID = String;

#[derive(Debug)]
/// StepArgument is an enum that represents the argument of a step command.
pub enum StepArgument {
  /// The argument is the value from the given port
  Port(PortID),
  /// The argument is a string
  String(String),
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

pub struct Port {
  pub port_ready: Notify,
  pub value: RwLock<PortData>,
}

impl Port {
  pub async fn set(&self, value: PortData) {
    *self.value.write().await = value;
  }

  pub async fn wait_for_data(&self) {
    if self.value.read().await.is_empty() {
      self.port_ready.notified().await;
    }

    if self.value.read().await.is_empty() {
      panic!("Port is empty, this should NOT happen");
    }
  }
}

pub struct Swirl {
  ports: Arc<HashMap<PortID, Port>>,
  orchestra: Arc<Orchestra>,
  workdir: PathBuf,
  connection_limit: Arc<tokio::sync::Semaphore>,
  pub amdahline: Arc<Amdahline>
}

impl Swirl {
  pub fn new(
    location: String,
    address_map: HashMap<String, LocationInfo>,
    workdir: PathBuf,
  ) -> Self {
    let mut ports = HashMap::new();

    // initialize data ports
    for port in PORTS {
      ports.insert(
        port.to_string(),
        Port {
          port_ready: Notify::new(),
          value: RwLock::new(PortData::Empty),
        },
      );
    }

    let orchestra = Arc::new(Orchestra::new(location.clone(), address_map));

    orchestra.accept_connections();

    Swirl {
      orchestra,
      ports: Arc::new(ports),
      workdir,
      connection_limit: Arc::new(tokio::sync::Semaphore::new(128)),
      amdahline: Arc::new(Amdahline::new(format!("amdahline/{}.log", location))),
    }
  }

  pub async fn init_port(&self, port: PortID, value: PortData) {
    let data = self.ports.get(&port).expect("port not found");
    data.set(value).await;
    data.port_ready.notify_waiters();
  }
}