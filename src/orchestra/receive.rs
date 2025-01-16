use std::sync::Arc;

use super::{LocationID, Orchestra, RelayTag};
use crate::orchestra::MessageHeader;
use bytes::Bytes;
use tokio::{
  io::{AsyncWrite, BufReader},
  net::TcpStream,
  task::{JoinHandle, JoinSet},
};

impl Orchestra {
  pub async fn receive_stream(
    self: &Arc<Self>,
    sender: LocationID,
    message_id: String,
  ) -> JoinHandle<(MessageHeader, BufReader<TcpStream>)> {
    let orchestra = self.clone();
    
    let handle: JoinHandle<(MessageHeader, BufReader<TcpStream>)> = tokio::spawn(async move {
      orchestra.blocking_receive_stream(sender, message_id).await
    });

    handle
  }

  pub async fn receive_stream_joinset(
    self: &Arc<Self>,
    sender: LocationID,
    message_id: String,
    mut join_set: JoinSet<(MessageHeader, BufReader<TcpStream>)>,
  ) -> JoinSet<(MessageHeader, BufReader<TcpStream>)> {
    let orchestra = self.clone();

    join_set.spawn(async move {
      orchestra.blocking_receive_stream(sender, message_id).await
    });

    join_set
  }

  pub async fn blocking_receive_stream(
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


  pub async fn blocking_receive_into<W>(
    self: &Arc<Self>,
    sender: LocationID,
    message_id: String,
    mut writer: W,
  ) -> (W, Bytes)
  where
    W: AsyncWrite + Unpin + Send + 'static,
  {
    let (header, mut reader) = self.blocking_receive_stream(sender, message_id.clone()).await;

    let header_data = Bytes::from(header.header_data);

    match header.relay_tag {
      RelayTag::Data() => {
        tokio::io::copy(&mut reader, &mut writer)
          .await
          .expect("failed to read message data");

        (writer, header_data)
      }
      RelayTag::Relay(relay_instructions) => {
        let writer = self
          .blocking_broadcast_relay(
            relay_instructions,
            message_id,
            reader,
            header_data.clone(),
            header.size,
            header.origin,
            Some(writer),
          )
          .await;

        (writer.unwrap(), header_data)
      }
    }
  }

  pub async fn receive_into<W>(
    self: &Arc<Self>,
    sender: LocationID,
    message_id: String,
    writer: W,
  ) -> JoinHandle<(W, Bytes)>
  where
    W: AsyncWrite + Unpin + Send + 'static,
  {
    let orchestra = self.clone();

    let handle = tokio::spawn(async move {
      orchestra.blocking_receive_into(sender, message_id, writer).await
    });

    handle
  }


  pub async fn receive_into_joinset<W>(
    self: &Arc<Self>,
    sender: LocationID,
    message_id: String,
    writer: W,
    mut join_set: JoinSet<(W, Bytes)>,
  ) -> JoinSet<(W, Bytes)>
  where
    W: AsyncWrite + Unpin + Send + 'static,
  {
    let orchestra = self.clone();

    join_set.spawn(async move {
      orchestra.blocking_receive_into(sender, message_id, writer).await
    });

    join_set
  }



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
