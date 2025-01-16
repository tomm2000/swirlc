use std::sync::Arc;

use tokio::task::JoinSet;

use crate::orchestra::{self, LocationID};

use super::{PortData, PortID, Swirl};

impl Swirl {
  pub async fn receive(
    self: &Arc<Self>,
    port_id: PortID,
    sender: String,
    mut join_set: JoinSet<()>,
  ) -> JoinSet<()> {
    println!("Receiving data from port {} from {}", port_id, sender);

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
      let (header, mut reader) = orchestra.blocking_receive_stream(sender, port_id.clone()).await;
      
      println!("Received stream from {}", sender);

      let received_port: PortData = bincode::deserialize(&header.header_data).expect("failed to deserialize header data");
  
      let port_data = swirl.ports.get(&port_id).expect("port not found");
  
      match received_port.clone() {
        PortData::Empty => {
          panic!("PortData::Empty should not be received");
        }
        PortData::File(file_path) => {
          let path = swirl.workdir.join(format!("receive_{}", orchestra.location_name(orchestra.location)));
          std::fs::create_dir_all(&path).expect(format!("failed to create directory {:?}", &path).as_str());
          let file_path = path.join(file_path);

          println!("received file: {:?}", file_path);
          
          let mut file = tokio::fs::File::create(&file_path).await.expect(format!("failed to create file: {:?}", &file_path).as_str());
  
          tokio::io::copy(&mut reader, &mut file)
            .await
            .expect("failed to write file data");
        }
        _ => { }
      }
  
      port_data.set(received_port.clone()).await;
      port_data.port_ready.notify_waiters();
    });

    join_set
  }
}
