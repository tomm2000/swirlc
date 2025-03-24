use super::{LocationID, Orchestra, RelayInstruction, RelayOptions};
use crate::orchestra::{
  utils::debug_prelude, MessageHeader, MESSAGE_CHUNK_SIZE, MESSAGE_HEADER_SIZE,
};

use std::{collections::HashMap, hash::Hash, sync::Arc, vec};

use bytes::Bytes;
use tokio::{
  io::{AsyncReadExt, AsyncWrite, AsyncWriteExt},
  net::TcpStream,
  task::{JoinHandle, JoinSet},
};

// TODO: for the broadcasts, instead of passing a vector of destinations, create an Into<Destinations> trait

/**
 * **DEPRECATED: Use `destination_ntree_advanced` instead.**
 * Generates a `RelayTag` describing a naive broadcast pattern.
 * `NAIVE`: The broadcasting node sends directly to each destination node.
 */
#[deprecated]
fn _destinations_naive(sender: LocationID, destinations: Vec<LocationID>) -> RelayInstruction {
  return RelayInstruction::Relay(
    destinations
      .iter()
      .map(|destination| RelayOptions {
        sender,
        destination: destination.clone(),
        relay_instruction: RelayInstruction::End,
      })
      .collect(),
  );
}

/**
 * **DEPRECATED: Use `destination_ntree_advanced` instead.**
 * Generates a `RelayTag` describing a n-tree broadcast pattern.
 * `NTREE`: The broadcasting node sends to n nodes, each of which sends to n other nodes, and so on.
 */
#[deprecated]
fn _destinations_ntree(sender: LocationID, destinations: &Vec<LocationID>, n: usize) -> RelayInstruction {
  let mut branches = Vec::new();

  for _ in 0..n {
    branches.push(Vec::new());
  }

  for (i, destination) in destinations.iter().enumerate() {
    branches[i % n].push(destination.clone());
  }

  let mut relay = Vec::new();

  for branch in branches.iter() {
    if branch.len() > 1 {
      let node = branch[0].clone();

      let remaining_tree = branch[1..].to_vec();

      // recursive instructions for the rest of the branch
      let recursive_instruction = _destinations_ntree(
        node,
        &remaining_tree,
        n
      );

      relay.push(RelayOptions {
        sender,
        destination: node,
        relay_instruction: recursive_instruction,
      });
      
    } else if branch.len() == 1 {
      let node = branch[0].clone();

      relay.push(RelayOptions {
        sender,
        destination: node,
        relay_instruction: RelayInstruction::End,
      });
    }
  }

  return RelayInstruction::Relay(relay);
}

/**
* Generates a `RelayTag` describing a more advanced broadcast pattern.
* `ADVANCED`: This broadcasting pattern works similar to the ntree pattern, but also accounts for the machine each node is on.
  the nodes are grouped per machine, and a master node is elected for each group. The broadcaster sends to each master node,
   and the master nodes send to the rest of the nodes in their group (following the ntree pattern).
*/
pub fn destinations_ntree_advanced(sender: LocationID, destinations: Vec<LocationID>, orchestra: &Orchestra) -> RelayInstruction {
  let master_machine = orchestra.location_info(sender).machine;

  let mut machine_groups: HashMap<String, Vec<LocationID>> = HashMap::new();

  machine_groups.insert(master_machine.clone(), vec![sender]);

  for destination in destinations {
    let machine = orchestra.location_info(destination).machine;
    
    if machine_groups.contains_key(&machine) {
      machine_groups.get_mut(&machine).unwrap().push(destination);
    } else {
      machine_groups.insert(machine, vec![destination]);
    }
  }

  // first node of each group is the master node
  let masters: Vec<LocationID> = machine_groups.iter().map(|(_, destinations)| {
    destinations[0].clone()
  }).collect();


  let slaves: HashMap<LocationID, Vec<LocationID>> = machine_groups.iter().map(|(_, destinations)| {
    let slaves = destinations[1..].to_vec();
    (destinations[0].clone(), slaves)
  }).collect();

  // remove sender from destinations
  let destinations: Vec<LocationID> = masters.into_iter().filter(|destination| *destination != sender).collect();
  let tree = destination_ntree_advanced_support(sender, &destinations, slaves, 2);

  return tree;
}

/**
 * Recursive support function to generate the advanced ntree pattern. See `destinations_ntree_advanced`.
 */
fn destination_ntree_advanced_support(sender: LocationID, destinations: &Vec<LocationID>, slaves: HashMap<LocationID, Vec<LocationID>>, n: usize) -> RelayInstruction {
  let mut branches: Vec<Vec<LocationID>> = vec![Vec::new(); n];

  for (i, destination) in destinations.iter().enumerate() {
    branches[i % n].push(destination.clone());
  }

  // remove empty branches
  branches = branches.into_iter().filter(|branch| branch.len() > 0).collect();

  let mut relay_options = Vec::new();

  for branch in branches.iter() {
    let node = branch[0].clone();

    let remaining_tree = if branch.len() > 1 {
      branch[1..].to_vec()
    } else {
      Vec::new()
    };

    // recursive instructions for the rest of the branch
    let recursive_instruction = destination_ntree_advanced_support(
      node,
      &remaining_tree,
      slaves.clone(),
      n
    );

    relay_options.push(RelayOptions {
      sender,
      destination: node,
      relay_instruction: recursive_instruction,
    });
  }

  // add the slave destinations
  if slaves.contains_key(&sender) {
    let slave_destinations = slaves.get(&sender).unwrap();

    for destination in slave_destinations {
      relay_options.push(RelayOptions {
        sender,
        destination: destination.clone(),
        relay_instruction: RelayInstruction::End,
      });
    }
  }

  return RelayInstruction::Relay(relay_options);
}


impl Orchestra {
  /**
  * Reads the data in the reader `R` and sends it to the destinations.
  * `header_data` is a byte array that can be used to send additional data with the message header.
   (note that there is a maximum size limit for the header, by default `MESSAGE_HEADER_SIZE` bytes).
  * `BLOCKING`: `.await` blocks the task until the whole message is sent.
  */
  pub async fn broadcast_blocking<R>(
    &self,
    destinations: Vec<LocationID>,
    message_id: String,
    reader: R,
    header_data: Bytes,
    data_size: usize,
  ) where
    R: AsyncReadExt + Unpin + Send + 'static,
  {
    // let instructions = destinations_naive(destinations);
    // let instructions = destinations_ntree(destinations, 2);
    let instructions = destinations_ntree_advanced(self.location.clone(), destinations, self);

    // println!(
    //   "{} Broadcasting message to {}",
    //   debug_prelude(&self.self_name(), None),
    //   instructions.display(self)
    // );

    match instructions {
      RelayInstruction::Relay(relay_instructions) => {
        self
          .broadcast_relay(
            relay_instructions,
            message_id,
            reader,
            header_data,
            data_size,
            self.location.clone(),
            tokio::io::empty(),
          )
          .await;
      }
      RelayInstruction::End => {
        panic!(
          "{} PANIC: no destinations",
          debug_prelude(&self.self_name(), None)
        );
      }
    }
  }

  /**
  * Reads the data in the reader `R` and sends it to the destinations.
  * `header_data` is a byte array that can be used to send additional data with the message header.
   (note that there is a maximum size limit for the header, by default `MESSAGE_HEADER_SIZE` bytes).
  * `NON-BLOCKING`: returns a `JoinHandle` that can be awaited to wait for completion.
  */
  pub fn broadcast<R>(
    self: &Arc<Self>,
    destinations: Vec<LocationID>,
    message_id: String,
    reader: R,
    header_data: Bytes,
    data_size: usize,
  ) -> JoinHandle<()>
  where
    R: AsyncReadExt + Unpin + Send + 'static,
  {
    let orchestra = self.clone();

    tokio::spawn(async move {
      orchestra
        .broadcast_blocking(destinations, message_id, reader, header_data, data_size)
        .await
    })
  }

  /**
  * Reads the data in the reader `R` and sends it to the destinations.
  * `header_data` is a byte array that can be used to send additional data with the message header.
   (note that there is a maximum size limit for the header, by default `MESSAGE_HEADER_SIZE` bytes).
  * `NON-BLOCKING`: adds a task to the `JoinSet` and returns the updated `JoinSet`.
  */
  pub fn broadcast_joinset<R>(
    self: &Arc<Self>,
    destinations: Vec<LocationID>,
    message_id: String,
    reader: R,
    header_data: Bytes,
    data_size: usize,
    mut join_set: JoinSet<()>,
  ) -> JoinSet<()>
  where
    R: AsyncReadExt + Unpin + Send + 'static,
  {
    let orchestra = self.clone();

    join_set.spawn(async move {
      orchestra
        .broadcast_blocking(destinations, message_id, reader, header_data, data_size)
        .await;
    });

    join_set
  }

  /**
   * **NOTE**: Support function, use `broadcast`, `broadcast_blocking`, or `broadcast_joinset` instead.
   * Relays the data from the reader `R` to the destinations specified in the `RelayTag`.
   * The data is also copied into the `read_into` parameter.
   * `BLOCKING`: `.await` blocks the task until the whole message is sent.
   */
  pub async fn broadcast_relay<R, W>(
    &self,
    relay_instructions: Vec<RelayOptions>,
    id: String,
    mut reader: R,
    header_data: Bytes,
    data_size: usize,
    origin: LocationID,
    mut read_into: W,
  ) -> W
  where
    R: AsyncReadExt + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
  {
    // ========= collect the addresses of the destinations =========
    let mut address_relay: Vec<(String, RelayInstruction)> = Vec::new();

    for instruction in relay_instructions {
      let location_info = self.addresses.get(&instruction.destination).expect(
        format!(
          "<Orchestra> unknown destination: {:?}",
          &instruction.destination
        )
        .as_str(),
      );
      address_relay.push((location_info.address.clone(), instruction.relay_instruction));
    }

    // ========= connect to the destinations =========
    let mut streams = Vec::new();

    for (address, tag) in address_relay {
      let mut stream;

      while {
        stream = TcpStream::connect(&address).await;
        stream.is_err()
      } {
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
      }

      let stream = stream.unwrap();
      streams.push((stream, tag));
    }

    // ========= write the message headers =========
    for (stream, tag) in streams.iter_mut() {
      let header_data = header_data.clone().to_vec();

      let message_header = MessageHeader {
        sender: self.location.clone(),
        origin: origin.clone(),
        message_id: id.clone(),
        size: data_size,
        relay_tag: tag.clone(),
        header_data,
      };

      let mut buffer = bincode::serialize(&message_header).unwrap();

      assert!(
        buffer.len() <= MESSAGE_HEADER_SIZE,
        "{} PANIC: message too large: {:?}",
        debug_prelude(&self.self_name(), None),
        buffer.len()
      );
      buffer.resize(MESSAGE_HEADER_SIZE, 0);

      stream
        .write_all(&buffer)
        .await
        .expect("failed to write message header");

      stream
        .flush()
        .await
        .expect("failed to flush message header");
    }

    // ========= write the message data =========
    let mut bytes: Vec<u8> = vec![0; MESSAGE_CHUNK_SIZE];

    while let Ok(size) = reader.read(&mut bytes).await {
      if size == 0 {
        break;
      }

      for (stream, _) in streams.iter_mut() {
        stream
          .write_all(&bytes[..size])
          .await
          .expect("failed to write message data");
      }

      read_into
        .write_all(&bytes[..size])
        .await
        .expect("failed to write message data");
    }

    for (stream, _) in streams.iter_mut() {
      stream.flush().await.expect("failed to flush message data");
      stream
        .shutdown()
        .await
        .expect("failed to shutdown message data");
    }

    read_into
      .flush()
      .await
      .expect("failed to flush message data");

    read_into
  }
}
