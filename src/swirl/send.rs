use std::{path::PathBuf, sync::Arc};

use bytes::Bytes;
use tokio::task::JoinSet;

use crate::orchestra::{self, utils::{debug_prelude, format_bytes}, LocationID};

use super::{PortData, PortID, Swirl};

impl Swirl {
  pub async fn send(
    self: &Arc<Self>,
    port_id: PortID,
    destination: String,
    mut join_set: JoinSet<()>,
  ) -> JoinSet<()> {
    let destination = self.orchestra.location_id(&destination);

    //============================ Copy Data ============================
    // copies the data to a local buffer to be sent
    // after the data is copied, the port can be modified and the send will not be affected
    //===================================================================
    let port = self.ports.get(&port_id).expect("port not found");

    port.wait_for_data().await;

    let data_read = port.value.read().await;
    let data = data_read.clone();

    let location = self.orchestra.location;
    let location = self.orchestra.location_name(location);

    drop(data_read);

    let handle = match data {
      PortData::File(path) => {
        let swirl = self.clone();

        join_set.spawn(async move {
          let permit = swirl.connection_limit.acquire_many(2).await.unwrap();

          let file_name = PathBuf::from(&path)
            .file_name()
            .expect("failed to get file name")
            .to_str().unwrap().to_string();

          let task = swirl.amdahline.begin_task(&location, &format!("send file {}", file_name));

          let file = tokio::fs::File::options()
            .read(true)
            .open(&path)
            .await
            .expect(format!("failed to open file: {:?}", path).as_str());

          let file_size = file.metadata().await.unwrap().len() as usize;

          let header_data = PortData::File(file_name);
          let header_data = bincode::serialize(&header_data).unwrap();
          let header_data = Bytes::from(header_data);

          println!("{} Sending file data to {}, size: {}", debug_prelude(&swirl.orchestra.self_name(), None), destination, format_bytes(file_size));

          swirl.orchestra.blocking_send(
            destination,
            port_id,
            file,
            header_data,
            file_size,
            swirl.orchestra.location
          ).await;

          swirl.amdahline.end_task(&location, task);

          drop(permit);
        });

        return join_set;
      }
      PortData::Empty => {
        println!("PANIC: empty data");
        panic!("empty data");
      }
      data => {
        let data = bincode::serialize(&data).unwrap();
        let size = data.len();

        println!("{} Sending data to {}, size: {}", debug_prelude(&self.orchestra.self_name(), None), destination, format_bytes(size));

        self.orchestra.send_joinset(
          destination,
          port_id,
          tokio::io::empty(),
          Bytes::from(data),
          size,
          join_set
        )
      }
    };

    println!("{} Completed send of data", debug_prelude(&self.orchestra.self_name(), None));

    return handle;
  }
}