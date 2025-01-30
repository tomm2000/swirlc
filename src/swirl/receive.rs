use std::sync::Arc;

use tokio::task::JoinSet;

use crate::orchestra::{
  self,
  utils::{format_bytes, debug_prelude},
  LocationID,
};

use super::{PortData, PortID, Swirl};

impl Swirl {
  pub async fn receive(
    self: &Arc<Self>,
    port_id: PortID,
    sender: String,
    mut join_set: JoinSet<()>,
  ) -> JoinSet<()> {
    println!(
      "{} Receiving data from port {} from {}",
      debug_prelude(&self.orchestra.self_name(), None),
      port_id,
      sender
    );

    let swirl = self.clone();
    let orchestra = self.orchestra.clone();
    let sender = orchestra.location_id(&sender);

    // clear the existing value
    self
      .ports
      .get(&port_id)
      .expect("port not found")
      .set(PortData::Empty)
      .await;

    join_set.spawn(async move {
      let received = orchestra.receive_blocking(sender, port_id.clone()).await;

      println!(
        "{} Receiving message from {}",
        debug_prelude(&orchestra.self_name(), None),
        sender
      );

      let received_port: PortData = bincode::deserialize(&received.header.header_data)
        .expect("failed to deserialize header data");

      let port_data = swirl.ports.get(&port_id).expect("port not found");

      match received_port.clone() {
        PortData::Empty => {
          panic!("PortData::Empty should not be received");
        }
        PortData::File(file_path) => {
          let path = swirl.workdir.join(format!(
            "receive_{}",
            orchestra.location_name(orchestra.location)
          ));
          let size = received.header.size;

          std::fs::create_dir_all(&path)
            .expect(format!("failed to create directory {:?}", &path).as_str());
          let full_path = path.join(&file_path);

          println!(
            "{} Receiving file into: {:?}, size: {}",
            debug_prelude(&orchestra.self_name(), None),
            full_path,
            format_bytes(size)
          );

          let file = tokio::fs::File::create(&full_path)
            .await
            .expect(format!("failed to create file: {:?}", &full_path).as_str());
          let writer = tokio::io::BufWriter::new(file);
          received.collect_blocking_into(writer).await;

          println!(
            "{} Received file: {:?}, size: {}",
            debug_prelude(&orchestra.self_name(), None),
            full_path,
            format_bytes(size)
          );

          port_data.set(PortData::File(full_path.to_str().unwrap().to_string())).await;
          port_data.port_ready.notify_waiters();
        }
        _ => {
          port_data.set(received_port.clone()).await;
          port_data.port_ready.notify_waiters();
        }
      }

    });

    join_set
  }
}
