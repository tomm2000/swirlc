pub mod broadcast;
pub mod receive;
pub mod send;
pub mod utils;

use std::{collections::HashMap, io::Read, net::{SocketAddr, SocketAddrV4}, str::FromStr, sync::Arc, task, thread};

use tokio::{
  io::{AsyncReadExt, BufReader},
  net::{TcpListener, TcpStream},
  sync::RwLock,
};
use utils::debug_prelude;

const MESSAGE_HEADER_SIZE: usize = 1024 * 2;
const MESSAGE_CHUNK_SIZE: usize = 8 * 1024 * 1024;

pub type LocationID = u16;

#[derive(serde::Serialize, serde::Deserialize, Hash, Eq, PartialEq, Debug, Clone)]
pub enum RelayInstruction {
  Relay(Vec<RelayOptions>),
  End,
}

#[derive(serde::Serialize, serde::Deserialize, Hash, Eq, PartialEq, Debug, Clone)]
pub struct RelayOptions {
  pub sender: LocationID,
  pub destination: LocationID,
  pub relay_instruction: RelayInstruction,
}

#[derive(serde::Serialize, serde::Deserialize, Hash, Eq, PartialEq, Debug, Clone)]
pub struct LocationInfo {
  pub address: String,
  pub machine: String,
}

impl RelayInstruction {
  pub fn display(&self, orchestra: &Orchestra) -> String {
    self.display_with_indent(orchestra, 0)
  }

  // Helper function to handle indentation
  pub fn display_with_indent(&self, orchestra: &Orchestra, indent: usize) -> String {
    let indent_str = "  ".repeat(indent);

    match self {
      RelayInstruction::End => {
        format!("{}End", indent_str)
      }
      RelayInstruction::Relay(instructions) => {
        let mut result = format!("{}Relay\n", indent_str);

        for instruction in instructions {
          // Get location name or ID if name not available
          let dest_name = orchestra.location_name(instruction.destination);

          // Add destination
          result.push_str(&format!("{}| → to {}\n", "  ".repeat(indent), dest_name));

          // Recursively display nested tree
          result.push_str(&instruction.relay_instruction.display_with_indent(orchestra, indent + 1));

          // Add newline between instructions unless it's the last one
          if instruction != instructions.last().unwrap() {
            result.push('\n');
          }
        }

        result
      }
    }
  }
}


#[derive(serde::Serialize, serde::Deserialize, Hash, Eq, PartialEq, Debug)]
pub struct MessageHeader {
  pub sender: LocationID,
  pub origin: LocationID,
  pub message_id: String,
  pub header_data: Vec<u8>,
  pub size: usize,
  pub relay_tag: RelayInstruction,
}

pub struct Orchestra {
  pub location: LocationID,
  addresses: HashMap<LocationID, LocationInfo>,
  locations: HashMap<String, LocationID>,
  incoming_messages:
    Arc<RwLock<HashMap<(LocationID, String), (MessageHeader, TcpStream)>>>,
}

unsafe impl Send for Orchestra {}

impl Orchestra {
  pub fn new(location: String, address_map: HashMap<String, LocationInfo>) -> Self {
    let mut addresses = HashMap::new();
    let mut locations = HashMap::new();

    // sort the address_map alphabetically by location
    let mut location_vec = address_map.keys().collect::<Vec<&String>>();
    location_vec.sort();

    for i in 0..location_vec.len() {
      let location = location_vec[i];
      let address = address_map.get(location).unwrap();

      addresses.insert(i as LocationID, address.clone());
      locations.insert(location.clone(), i as LocationID);
    }

    let location: LocationID = *locations.get(&location).unwrap();

    Self {
      locations,
      addresses,
      location,
      incoming_messages: Arc::new(RwLock::new(HashMap::new())),
    }
  }

  // TODO: clean this up
  pub fn locations(&self) -> &HashMap<String, LocationID> {
    &self.locations
  }

  pub fn location_ids(&self) -> Vec<LocationID> {
    self.locations.values().map(|id| *id).collect()
  }

  pub fn location_id(&self, location: &str) -> LocationID {
    *self
      .locations
      .get(location)
      .expect(format!("Location {} not found", location).as_str())
  }

  pub fn location_name(&self, location_id: LocationID) -> String {
    self
      .locations
      .iter()
      .find(|(_, id)| **id == location_id)
      .unwrap()
      .0
      .clone()
  }

  pub fn self_name(&self) -> String {
    self.location_name(self.location)
  }

  pub fn location_info(&self, location: LocationID) -> LocationInfo {
    self.addresses.get(&location).unwrap().clone()
  }

  /**
  * Spawns a task accepting incoming connections from other locations in a loop,
   abort the handle to close the listener.
  * `NON-BLOCKING`: this function spawns a task that listens for incoming connections, immediately returning a handle to the task.
  */
  pub fn accept_connections(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
    let orchestra = self.clone();

    tokio::spawn(async move {
      let location_info = orchestra.addresses.get(&orchestra.location).unwrap();

      let listener = TcpListener::bind(&location_info.address).await;

      if let Err(e) = listener {
        println!(
          "{} failed to bind to address {:?} with error {:?}",
          debug_prelude(&orchestra.self_name(), None),
          location_info,
          e
        );
        return;
      }

      let listener = listener.unwrap();

      println!(
        "{} Listening on {:?}",
        debug_prelude(&orchestra.self_name(), None),
        location_info
      );

      loop {
        let (stream, _) = listener
          .accept()
          .await
          .expect("failed to accept connection");

        tokio::spawn({
          let orchestra = orchestra.clone();

          async move {
            Self::handle_connection(orchestra, stream).await;
          }
        });
      }
    })
  }

  async fn handle_connection(orchestra: Arc<Self>, mut stream: TcpStream) {
    let mut buffer = vec![0; MESSAGE_HEADER_SIZE];
    stream
      .read_exact(&mut buffer)
      .await
      .expect("failed to read message");

    let message_header: MessageHeader = bincode::deserialize(&buffer).unwrap();

    // println!(
    //   "{} Received message (tag: {:?}) from {:?} origin {:?}",
    //   debug_prelude(&orchestra.self_name(), None),
    //   message_header.relay_tag,
    //   message_header.sender,
    //   message_header.origin
    // );

    orchestra
      .incoming_messages
      .write()
      .await
      .insert(
        (
          message_header.origin.clone(),
          message_header.message_id.clone(),
        ),
        (message_header, stream),
      );
  }
}