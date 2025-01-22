use super::{LocationID, Orchestra, RelayInstruction, RelayTag};
use crate::orchestra::{
  utils::debug_prelude, MessageHeader, MESSAGE_CHUNK_SIZE, MESSAGE_HEADER_SIZE,
};

use std::{collections::HashMap, sync::Arc, vec};

use bytes::Bytes;
use tokio::{
  io::{AsyncReadExt, AsyncWrite, AsyncWriteExt},
  net::TcpStream,
  task::{JoinHandle, JoinSet},
};

// TODO: for the broadcasts, instead of passing a vector of destinations, create an Into<Destinations> trait

/**
 * Generates a `RelayTag` describing a naive broadcast pattern.
 * `NAIVE`: The broadcasting node sends directly to each destination node.
 */
fn destinations_naive(destinations: Vec<LocationID>) -> RelayTag {
  return RelayTag::Relay(
    destinations
      .iter()
      .map(|destination| RelayInstruction {
        destination: destination.clone(),
        tag: RelayTag::Data(),
      })
      .collect(),
  );
}

/**
 * Generates a `RelayTag` describing a n-tree broadcast pattern.
 * `NTREE`: The broadcasting node sends to n nodes, each of which sends to n other nodes, and so on.
 */
fn destinations_ntree(destinations: Vec<LocationID>, n: usize) -> RelayTag {
  let mut parts = Vec::new();

  for _ in 0..n {
    parts.push(Vec::new());
  }

  for (i, destination) in destinations.iter().enumerate() {
    parts[i % n].push(destination.clone());
  }

  let mut destination_tags: Vec<RelayInstruction> = Vec::new();

  for part in parts.iter() {
    if part.len() > 1 {
      let tag = destinations_ntree(part[1..].to_vec(), n);

      destination_tags.push(RelayInstruction {
        destination: part[0].clone(),
        tag,
      });
    } else if part.len() == 1 {
      destination_tags.push(RelayInstruction {
        destination: part[0].clone(),
        tag: RelayTag::Data(),
      });
    }
  }

  RelayTag::Relay(destination_tags)
}

/**
* Generates a `RelayTag` describing a more advanced broadcast pattern.
* `ADVANCED`: This broadcasting pattern works similar to the ntree pattern, but also accounts for the machine each node is on.
  the nodes are grouped per machine, and a master node is elected for each group. The broadcaster sends to each master node,
   and the master nodes send to the rest of the nodes in their group (following the ntree pattern).
*/
fn destination_ntree_advanced(node: LocationID, destinations: Vec<LocationID>, orchestra: &Orchestra) -> RelayTag {
  // println!("broadcasting from node: {:?}", node);

  let master_machine = orchestra
    .addresses
    .get(&node)
    .expect(format!("<Orchestra> unknown node: {:?}", node).as_str())
    .machine
    .clone();

  // println!("machine: {:?}", master_machine);

  let mut slaves: Vec<LocationID> = Vec::new();

  // find the locations on the same machine as node, and remove node from the list
  let other_destinations = destinations.iter().filter(|destination| {
    let location_info = orchestra
      .addresses
      .get(destination)
      .expect(format!("<Orchestra> unknown destination: {:?}", destination).as_str());
    let machine = location_info.machine.clone();

    if master_machine == machine {
      slaves.push((*destination).clone());
      return false;
    }

    return true;
  }).map(|id| id.clone()).collect::<Vec<LocationID>>();

  // println!("slaves: {:?}, other_destinations: {:?}", slaves, other_destinations);

  let mut slave_instructions: Vec<RelayInstruction> = slaves.iter().map(|destination| {
    RelayInstruction {
      destination: destination.clone(),
      tag: RelayTag::Data(),
    }
  }).collect();

  if other_destinations.len() == 0 {
    return RelayTag::Relay(slave_instructions);
  }

  // if there are more destinations, find the next node to send to
  let next_node = other_destinations[0].clone();
  let other_destinations = other_destinations[1..].to_vec();
  let next_tag = destination_ntree_advanced(next_node.clone(), other_destinations, orchestra);

  slave_instructions.push(RelayInstruction {
    destination: next_node.clone(),
    tag: next_tag
  });

  return RelayTag::Relay(slave_instructions);
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
    let instructions = destination_ntree_advanced(self.location.clone(), destinations, self);

    // println!(
    //   "{} Broadcasting message to {}",
    //   debug_prelude(&self.self_name(), None),
    //   instructions.display(self)
    // );

    match instructions {
      RelayTag::Relay(relay_instructions) => {
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
      RelayTag::Data() => {
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
    relay_instructions: Vec<RelayInstruction>,
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
    let mut address_relay: Vec<(String, RelayTag)> = Vec::new();

    for instruction in relay_instructions {
      let location_info = self.addresses.get(&instruction.destination).expect(
        format!(
          "<Orchestra> unknown destination: {:?}",
          &instruction.destination
        )
        .as_str(),
      );
      address_relay.push((location_info.address.clone(), instruction.tag));
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
