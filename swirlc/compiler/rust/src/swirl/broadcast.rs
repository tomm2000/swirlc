use std::{path::PathBuf, sync::Arc};

use bytes::Bytes;
use tokio::{io::BufReader, task::JoinSet};

use crate::orchestra::{utils::debug_prelude, LocationID};

use super::{PortData, PortID, Swirl};

impl Swirl {
  pub async fn broadcast(
    self: &Arc<Self>,
    port_id: PortID,
    destinations: Vec<String>,
    mut join_set: JoinSet<()>,
  ) -> JoinSet<()> {
    let destinations = destinations
      .iter()
      .map(|d| self.orchestra.location_id(d))
      .collect::<Vec<LocationID>>();

    let location = self.orchestra.location;
    let location = self.orchestra.location_name(location);

    let port = self.ports.get(&port_id).expect("port not found");

    port.wait_for_data().await;

    let data_read = port.value.read().await;
    let data = data_read.clone();

    drop(data_read);

    match data {
      PortData::File(path) => {
        let swirl = self.clone();

        let required_permits = 1 + destinations.len() as u32;

        join_set.spawn(async move {
          let permit = swirl.connection_limit.acquire_many(required_permits).await;

          let file_name = PathBuf::from(&path)
            .file_name()
            .expect("failed to get file name")
            .to_str()
            .unwrap()
            .to_string();

          let task = swirl.amdahline.begin_task(&location, &format!("broadcast file {}", file_name));

          let file = tokio::fs::File::options()
            .read(true)
            .open(&path)
            .await
            .expect(format!("failed to open file: {:?}", path).as_str());

          let file_size = file.metadata().await.unwrap().len() as usize;


          let header_data = PortData::File(file_name);
          let header_data = bincode::serialize(&header_data).unwrap();
          let header_data = Bytes::from(header_data);

          let reader = BufReader::new(file);

          swirl
            .orchestra
            .broadcast_blocking(destinations, port_id, reader, header_data, file_size)
            .await;

          println!(
            "{} Completed broadcast of file data",
            debug_prelude(&swirl.orchestra.self_name(), None)
          );

          swirl.amdahline.end_task(&location, task);

          drop(permit);
        });

        return join_set;
      }
      PortData::Empty => {
        println!(
          "{} PANIC: empty data",
          debug_prelude(&self.orchestra.self_name(), None)
        );
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
          join_set,
        );

        println!(
          "{} Completed broadcast of data",
          debug_prelude(&self.orchestra.self_name(), None)
        );

        return join_set;
      }
    };
  }
}
