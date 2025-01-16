use std::sync::Arc;

use super::{LocationID, Orchestra, RelayTag};
use crate::orchestra::MessageHeader;
use bytes::Bytes;
use tokio::{
  io::{AsyncWrite, BufReader},
  net::TcpStream,
  task::{JoinHandle, JoinSet},
};

pub struct PartialReceive {
  pub header: MessageHeader,
  pub reader: BufReader<TcpStream>,
  orchestra: Arc<Orchestra>,
}

impl PartialReceive {
  pub async fn receive_blocking_into<W>(mut self, mut writer: W) -> W where W: AsyncWrite + Unpin + Send + 'static {
    match self.header.relay_tag.clone() {
      RelayTag::Data() => {
        tokio::io::copy(&mut self.reader, &mut writer)
          .await
          .expect("failed to read message data");
      }
      RelayTag::Relay(relay_instructions) => {
        let writer = self.orchestra
          .blocking_broadcast_relay(
            relay_instructions,
            self.header.message_id.clone(),
            self.reader,
            Bytes::from(self.header.header_data.clone()),
            self.header.size,
            self.header.origin,
            writer,
          ).await;

        return writer;
      }
    }

    return writer;
  }

  pub async fn receive_into<W>(self, writer: W) -> JoinHandle<W> where W: AsyncWrite + Unpin + Send + 'static {
    tokio::spawn(async move {
      self.receive_blocking_into(writer).await
    })
  }

  pub async fn receive_joinset_into<W>(self, writer: W, mut join_set: JoinSet<W>) -> JoinSet<W> where W: AsyncWrite + Unpin + Send + 'static {
    join_set.spawn(async move {
      self.receive_blocking_into(writer).await
    });

    join_set
  }
}

impl Orchestra {
  async fn collect_message(
    self: &Arc<Self>,
    sender: LocationID,
    message_id: String,
  ) -> (MessageHeader, BufReader<TcpStream>) {
    loop {
      let incoming_messages = self.incoming_messages.read().await;

      if incoming_messages.contains_key(&(sender, message_id.clone())) {
        drop(incoming_messages);

        let message = self
          .incoming_messages
          .write()
          .await
          .remove(&(sender, message_id))
          .unwrap();

        return message;
      }

      drop(incoming_messages);
      tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }
  }

  pub async fn receive_blocking(
    self: &Arc<Self>,
    sender: LocationID,
    message_id: String,
  ) -> PartialReceive {
    let (header, reader) = self.collect_message(sender, message_id).await;

    PartialReceive { header, reader, orchestra: self.clone() }
  }

  pub fn receive(
    self: &Arc<Self>,
    sender: LocationID,
    message_id: String,
  ) -> JoinHandle<PartialReceive> {
    let orchestra = self.clone();

    tokio::spawn(async move {
      orchestra.receive_blocking(sender, message_id).await
    })
  }

  pub fn receive_joinset(
    self: &Arc<Self>,
    sender: LocationID,
    message_id: String,
    mut join_set: JoinSet<PartialReceive>,
  ) -> JoinSet<PartialReceive> {
    let orchestra = self.clone();

    join_set.spawn(async move {
      orchestra.receive_blocking(sender, message_id).await
    });

    join_set
  }

  // pub async fn receive(
  //   self: &Arc<Self>,
  //   sender: LocationID,
  //   message_id: String,
  // ) -> PartialReceive {
  //   let (header, reader) = self.collect_message(sender, message_id).await;

  //   PartialReceive { header, reader, orchestra: self.clone() }
  // }

  // pub async fn receive_joinset(
  //   self: &Arc<Self>,
  //   sender: LocationID,
  //   message_id: String,
  // ) -> PartialReceive {
  //   let (header, reader) = self.collect_message(sender, message_id).await;

  //   PartialReceive { header, reader, orchestra: self.clone() }
  // }

  // pub async fn receive_into<W>(
  //   &self,
  //   sender: LocationID,
  //   message_id: String,
  //   mut writer: W,
  // ) -> (W, Bytes)
  // where
  //   W: AsyncWrite + Unpin + Send + 'static,
  // {
  //   let (header, mut reader) = self.receive_stream(sender, message_id.clone()).await;

  //   let header_data = Bytes::from(header.header_data);

  //   match header.relay_tag {
  //     RelayTag::Data() => {
  //       tokio::io::copy(&mut reader, &mut writer)
  //         .await
  //         .expect("failed to read message data");

  //       (writer, header_data)
  //     }
  //     RelayTag::Relay(relay_instructions) => {
  //       let writer = self
  //         .blocking_broadcast_relay(
  //           relay_instructions,
  //           message_id,
  //           reader,
  //           header_data.clone(),
  //           header.size,
  //           header.origin,
  //           Some(writer),
  //         )
  //         .await;

  //       (writer.unwrap(), header_data)
  //     }
  //   }
  // }

  // pub async fn receive_vecu8(&self, sender: LocationID, id: String) -> (Vec<u8>, Bytes) {
  //   let data = Vec::new();
  //   let writer = BufWriter::new(data);

  //   let (mut writer, header_data) = self.receive_into(sender, id, writer).await;

  //   writer.flush().await.expect("failed to flush writer");

  //   (writer.into_inner(), header_data)
  // }

  // pub async fn receive_string(&self, sender: LocationID, id: String) -> (String, Bytes) {
  //   let (data, header_data) = self.receive_vecu8(sender, id).await;

  //   (String::from_utf8(data).unwrap(), header_data)
  // }

  // pub async fn receive_file(&self, sender: LocationID, id: String, path: &str) {
  //   let file = tokio::fs::OpenOptions::new()
  //     .write(true)
  //     .create(true)
  //     .open(path)
  //     .await
  //     .expect("failed to open file");
  //   let writer = BufWriter::new(file);

  //   self.receive_into(sender, id, writer).await;
  // }
}
