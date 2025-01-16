use crate::orchestra::{utils::debug_prelude, MessageHeader, MESSAGE_CHUNK_SIZE, MESSAGE_HEADER_SIZE};
use super::{LocationID, Orchestra, RelayInstruction, RelayTag};

use std::{collections::HashMap, sync::Arc, vec};

use bytes::Bytes;
use tokio::{io::{AsyncReadExt, AsyncWrite, AsyncWriteExt}, net::TcpStream, task::JoinHandle};


// fn destinations_logarithmic(destinations: Vec<LocationID>, data_size: usize) -> Vec<(LocationID, MessageTag)> {
//   let mut destinations = destinations.clone();

//   let n_steps = (destinations.len() as f64 + 1.0).log2().ceil() as usize;

//   let mut parts = Vec::new();

//   // divide the destionation in 2 parts over and over again
//   for _ in 0..n_steps {
//     let part_size = destinations.len() / 2 + 1;
//     let part_size = if part_size > destinations.len() { destinations.len() } else { part_size };
//     let mut part = Vec::new();

//     for _ in 0..part_size {
//       part.push(destinations.pop().unwrap());
//     }

//     parts.push(part);
//   }

//   let mut destination_tags: Vec<(LocationID, MessageTag)> = Vec::new();

//   // the first n-1 parts will be relay tags
//   for part in parts.iter().take(parts.len() - 1) {
//     // the message should be relayed to the 1:part.len() destinations (first location is excluded)
//     let tag = MessageTag::Relay(data_size, part[1..].to_vec());

//     destination_tags.push((part[0], tag));
//   }

//   // the last part will be a data tag
//   for destination in parts.last().unwrap() {
//     let tag = MessageTag::Data(data_size);

//     destination_tags.push((*destination, tag));
//   }

//   destination_tags
// }

fn destination_ntree_advanced(destinations: Vec<LocationID>, orchestra: &Orchestra) -> RelayTag {
  let mut location_map: HashMap<String, Vec<LocationID>> = HashMap::new();

  destinations.iter().for_each(|destination| {
    let location_info = orchestra.addresses.get(destination).expect(format!("<Orchestra> unknown destination: {:?}", destination).as_str());
    let machine = location_info.machine.clone();

    if location_map.contains_key(&machine) {
      location_map.get_mut(&machine).unwrap().push(destination.clone());
    } else {
      location_map.insert(machine, vec![destination.clone()]);
    }
  });

  // TODO: also transform the master nodes into a relay chain

  let mut relay_instructions: Vec<RelayInstruction> = Vec::new();

  for (machine, destinations) in location_map.iter() {
    let machine_master = destinations[0].clone();
    let machine_slaves = destinations[1..].to_vec();

    println!("machine: {:?}, master: {:?}, slaves: {:?}", machine, machine_master, machine_slaves);

    if destinations.len() > 1 {
      let tag = destinations_ntree(machine_slaves, 1);

      relay_instructions.push(RelayInstruction {
        destination: machine_master,
        tag
      });

    } else if destinations.len() == 1 {
      relay_instructions.push(RelayInstruction {
        destination: machine_master,
        tag: RelayTag::Data()
      });
    }
  }
  
  RelayTag::Relay(relay_instructions)
}

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
        tag
      });
    } else if part.len() == 1 {
      destination_tags.push(RelayInstruction {
        destination: part[0].clone(),
        tag: RelayTag::Data()
      });
    }
  }

  RelayTag::Relay(destination_tags)
}

fn destinations_naive(destinations: Vec<LocationID>) -> RelayTag {
  return RelayTag::Relay(
    destinations.iter().map(|destination| {
      RelayInstruction {
        destination: destination.clone(),
        tag: RelayTag::Data()
      }
    }).collect()
  )
}

impl Orchestra {
  pub async fn broadcast<R, W>(
    self: &Arc<Self>,
    destinations: Vec<LocationID>,
    message_id: String,
    reader: R,
    header_data: Bytes,
    data_size: usize,
  ) -> JoinHandle<Option<W>>
  where
    R: AsyncReadExt + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static
  {
    let instructions = destinations_ntree(destinations, 2);

    match instructions {
      RelayTag::Relay(relay_instructions) => {
        let l = self.broadcast_relay(relay_instructions, message_id, reader, header_data, data_size, self.location.clone(), None::<W>).await;
        return l;
      }
      RelayTag::Data() => {
        panic!("{} PANIC: no destinations", debug_prelude(&self.self_name(), None));
      }
    };
  }

  pub async fn broadcast_relay<R, W>(
    self: &Arc<Self>,
    relay_instructions: Vec<RelayInstruction>,
    message_id: String,
    reader: R,
    header_data: Bytes,
    data_size: usize,
    origin: LocationID,
    read_into: Option<W>
  ) -> JoinHandle<Option<W>>
  where
    R: AsyncReadExt + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static
  {
    let orchestra = self.clone();

    tokio::spawn(async move {
      orchestra.blocking_broadcast_relay(relay_instructions, message_id, reader, header_data, data_size, origin, read_into).await
    })
  }

  pub async fn blocking_broadcast<R>(
    &self,
    destinations: Vec<LocationID>,
    message_id: String,
    reader: R,
    header_data: Bytes,
    data_size: usize,
  ) where R: AsyncReadExt + Unpin + Send + 'static {
    // let instructions = destinations_naive(destinations);
    // let instructions = destinations_ntree(destinations, 2);
    let instructions = destination_ntree_advanced(destinations, &self);

    println!("{}", instructions.display(self));

    match instructions {
      RelayTag::Relay(relay_instructions) => {
        self.blocking_broadcast_relay(relay_instructions, message_id, reader, header_data, data_size, self.location.clone(), None::<Vec<u8>>).await;
      }
      RelayTag::Data() => {
        panic!("{} PANIC: no destinations", debug_prelude(&self.self_name(), None));
      }
    }
  }

  pub async fn blocking_broadcast_relay<R, W>(&self,
    relay_instructions: Vec<RelayInstruction>,
    id: String,
    mut reader: R,
    header_data: Bytes,
    data_size: usize,
    origin: LocationID,
    mut read_into: Option<W>
  ) -> Option<W>
  where
    R: AsyncReadExt + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static
  {
    // ========= collect the addresses of the destinations =========
    let mut address_relay: Vec<(String, RelayTag)> = Vec::new();

    for instruction in relay_instructions {
      let location_info = self.addresses.get(&instruction.destination).expect(format!("<Orchestra> unknown destination: {:?}", &instruction.destination).as_str());
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
        header_data
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

      stream.flush().await.expect("failed to flush message header");
    }

    // ========= write the message data =========
    let mut bytes: Vec<u8> = vec![0; MESSAGE_CHUNK_SIZE];

    while let Ok(size) = reader.read(&mut bytes).await {
      if size == 0 { break; }

      for (stream, _) in streams.iter_mut() {
        stream.write_all(&bytes[..size]).await.expect("failed to write message data");
      }

      if read_into.is_some() {
        read_into.as_mut().unwrap().write_all(&bytes[..size]).await.expect("failed to write message data");
      }
    }

    for (stream, _) in streams.iter_mut() {
      stream.flush().await.expect("failed to flush message data");
      stream.shutdown().await.expect("failed to shutdown message data");
    }

    // println!("{} communicated to {} locations", debug_prelude(&self.location, None), streams.len());

    read_into
  }
}