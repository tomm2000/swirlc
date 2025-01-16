use std::{path::PathBuf, sync::Arc};

use bytes::Bytes;
use tokio::{io::BufReader, task::JoinSet};

use crate::orchestra::LocationID;

use super::{PortData, PortID, Swirl};

impl Swirl {
  pub async fn broadcast(
    self: &Arc<Self>,
    port_id: PortID,
    destinations: Vec<String>,
    join_set: JoinSet<()>,
  ) -> JoinSet<()> {
    let destinations = destinations
      .iter()
      .map(|d| self.orchestra.location_id(d))
      .collect::<Vec<LocationID>>();

    let port = self.ports.get(&port_id).expect("port not found");

    port.wait_for_data().await;

    let data_read = port.value.read().await;
    let data = data_read.clone();

    drop(data_read);

    match data {
      PortData::File(path) => {
        let file = tokio::fs::File::options()
          .read(true)
          .open(&path)
          .await
          .expect(format!("failed to open file: {:?}", path).as_str());

        let file_size = file.metadata().await.unwrap().len() as usize;

        let file_name = PathBuf::from(&path)
          .file_name()
          .expect("failed to get file name")
          .to_str().unwrap().to_string();

        let header_data = PortData::File(file_name);
        let header_data = bincode::serialize(&header_data).unwrap();
        let header_data = Bytes::from(header_data);

        let reader = BufReader::new(file);

        let join_set = self.orchestra.broadcast_joinset(
          destinations,
          port_id,
          reader,
          header_data,
          file_size,
          join_set
        );

        println!("Sent file data");

        return join_set;
      }
      PortData::Empty => {
        println!("PANIC: empty data");
        panic!("empty data");
      }
      data => {
        let data = bincode::serialize(&data).unwrap();
        let data_size = data.len();

        let join_set = self.orchestra.broadcast_joinset(
          destinations,
          port_id,
          tokio::io::empty(),
          Bytes::from(data),
          data_size,
          join_set
        );

        println!("Sent data");

        return join_set;
      }
    };
  }
}