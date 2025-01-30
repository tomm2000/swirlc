use std::sync::Arc;

use super::{LocationID, Orchestra, RelayTag};
use crate::orchestra::MessageHeader;
use bytes::Bytes;
use tokio::{
  io::{AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader, BufWriter},
  net::TcpStream,
  task::{JoinHandle, JoinSet},
};

/**
 * PartialReceive is a struct that is returned when receiving a message.
 * It can be used to read the message header immediately and then (optionally) collect the message data.
 * The `collect_*` methods are used to collect the message data into a specific type.
 */
pub struct PartialReceive {
  pub header: MessageHeader,
  pub stream: TcpStream,
  orchestra: Arc<Orchestra>,
}

impl PartialReceive {
  // ==================== Receive into ====================
  pub async fn collect_blocking_into<W>(self, mut writer: W) -> W where W: AsyncWrite + Unpin + Send + 'static {
    let mut reader = BufReader::with_capacity(1024*1024*32, self.stream);

    match self.header.relay_tag.clone() {
      RelayTag::Data() => {
        tokio::io::copy(&mut reader, &mut writer)
          .await
          .expect("failed to read message data");
      }
      RelayTag::Relay(relay_instructions) => {
        let writer = self.orchestra
          .broadcast_relay(
            relay_instructions,
            self.header.message_id.clone(),
            reader,
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

  pub fn collect_into<W>(self, writer: W) -> JoinHandle<W> where W: AsyncWrite + Unpin + Send + 'static {
    tokio::spawn(async move {
      self.collect_blocking_into(writer).await
    })
  }

  pub fn collect_joinset_into<W>(self, writer: W, mut join_set: JoinSet<W>) -> JoinSet<W> where W: AsyncWrite + Unpin + Send + 'static {
    join_set.spawn(async move {
      self.collect_blocking_into(writer).await
    });

    join_set
  }
  // ======================================================

  // ==================== Receive Vec<u8> =================
  pub async fn collect_blocking_vecu8(self) -> Vec<u8> {
    let data = Vec::with_capacity(self.header.size as usize);
    let writer = BufWriter::new(data);

    let mut writer = self.collect_blocking_into(writer).await;

    writer.flush().await.expect("failed to flush writer");

    let data = writer.into_inner();

    data
  }

  pub fn collect_vecu8(self) -> JoinHandle<Vec<u8>> {
    tokio::spawn(async move {
      self.collect_blocking_vecu8().await
    })
  }

  pub fn collect_joinset_vecu8(self, mut join_set: JoinSet<Vec<u8>>) -> JoinSet<Vec<u8>> {
    join_set.spawn(async move {
      self.collect_blocking_vecu8().await
    });

    join_set
  }
  // ======================================================

  // ==================== Receive String =================
  pub async fn collect_blocking_string(self) -> String {
    let data = self.collect_blocking_vecu8().await;

    String::from_utf8(data).unwrap()
  }

  pub fn collect_string(self) -> JoinHandle<String> {
    tokio::spawn(async move {
      self.collect_blocking_string().await
    })
  }

  pub fn collect_joinset_string(self, mut join_set: JoinSet<String>) -> JoinSet<String> {
    join_set.spawn(async move {
      self.collect_blocking_string().await
    });

    join_set
  }
  // ======================================================

  // ==================== Receive File ===================
  pub async fn collect_blocking_file<P>(self, path: P) where P: AsRef<std::path::Path> {
    let file = tokio::fs::OpenOptions::new()
      .write(true)
      .create(true)
      .open(path)
      .await
      .expect("failed to open file");
    let writer = tokio::io::BufWriter::new(file);

    let mut writer = self.collect_blocking_into(writer).await;
    writer.shutdown().await.expect("failed to shutdown writer");
  }

  pub fn collect_file<P>(self, path: P) -> JoinHandle<()> where P: AsRef<std::path::Path> + Send + 'static {
    tokio::spawn(async move {
      self.collect_blocking_file(path).await
    })
  }

  pub fn collect_joinset_file<P>(self, path: P, mut join_set: JoinSet<()>) -> JoinSet<()> where P: AsRef<std::path::Path> + Send + 'static {
    join_set.spawn(async move {
      self.collect_blocking_file(path).await
    });

    join_set
  }
  // ======================================================
}

impl Orchestra {
  /**
   * Fetches a message from the incoming messages buffer.
   * `.await` blocks until the message is available.
   */
  async fn fetch_message(
    self: &Arc<Self>,
    sender: LocationID,
    message_id: String,
  ) -> (MessageHeader, TcpStream) {
    loop {
      let incoming_messages = self.incoming_messages.read().await;

      if incoming_messages.contains_key(&(sender, message_id.clone())) {
        drop(incoming_messages);

        let (header, stream) = self
          .incoming_messages
          .write()
          .await
          .remove(&(sender, message_id))
          .unwrap();

        return (header, stream);
      }

      drop(incoming_messages);
      tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }
  }

  /**
   * Receives the message header from a specific sender, the returned `PartialReceive` can be used to collect the message data.
   * `BLOCKING`: `.await` blocks the task until the message is available.
   */
  pub async fn receive_blocking(
    self: &Arc<Self>,
    sender: LocationID,
    message_id: String,
  ) -> PartialReceive {
    let (header, reader) = self.fetch_message(sender, message_id).await;

    PartialReceive { header, stream: reader, orchestra: self.clone() }
  }

  /**
   * Receives the message header from a specific sender, the returned `PartialReceive` can be used to collect the message data.
   * `NON-BLOCKING`: join the returned `JoinHandle` to wait for completion.
   */
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

  /**
   * Receives the message header from a specific sender, the returned `PartialReceive` can be used to collect the message data.
   * `NON-BLOCKING`: adds a task to the `JoinSet` and returns the updated `JoinSet`.
   */
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
}
