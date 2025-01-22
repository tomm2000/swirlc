use crate::orchestra::{utils::debug_prelude, MessageHeader, RelayTag, MESSAGE_HEADER_SIZE};
use super::{LocationID, Orchestra};

use std::sync::Arc;

use bytes::Bytes;
use tokio::{io::{AsyncReadExt, AsyncWriteExt, BufReader, BufWriter}, net::TcpStream, task::{JoinHandle, JoinSet}};

impl Orchestra {
  /**
   * Reads the data in the reader `R` and sends it to the destination.
   * `header_data` is a byte array that can be used to send additional data with the message header.
    (note that there is a maximum size limit for the header, by default `MESSAGE_HEADER_SIZE` bytes).
   * `BLOCKING`: `.await` blocks the task until the whole message is sent.
   */
  pub async fn blocking_send<R>(
    self: &Arc<Self>,
    destination: LocationID,
    message_id: String,
    reader: R,
    header_data: Bytes,
    data_size: usize,
    origin: LocationID
  )
    where R: AsyncReadExt + Unpin + Send + 'static
  {
    let location_info = self.addresses.get(&destination).expect(format!("<Orchestra> unknown destination: {:?}", &destination).as_str());

    let mut stream;

    while {
      stream = TcpStream::connect(&location_info.address).await;
      stream.is_err()
    } {
      tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }

    let stream = stream.unwrap();
    let mut writer = BufWriter::new(stream);

    // === Write message header ===
    let message_header = MessageHeader {
      sender: self.location.clone(),
      origin,
      message_id,
      size: data_size,
      relay_tag: RelayTag::Data(),
      header_data: header_data.to_vec()
    };

    let mut buffer = bincode::serialize(&message_header).unwrap();

    assert!(
      buffer.len() <= MESSAGE_HEADER_SIZE,
      "{} PANIC: message too large: {:?}",
      debug_prelude(&self.location, None),
      buffer.len()
    );
    buffer.resize(MESSAGE_HEADER_SIZE, 0);

    writer
      .write_all(&buffer)
      .await
      .expect("failed to write message header");

    writer.flush().await.expect("failed to flush message header");

    // === Write message data ===
    let mut reader = BufReader::new(reader);
    tokio::io::copy(&mut reader, &mut writer).await.expect("failed to copy message data");

    writer.flush().await.expect("failed to flush message data");
  }

  /**
   * Reads the data in the reader `R` and sends it to the destination.
   * `header_data` is a byte array that can be used to send additional data with the message header.
    (note that there is a maximum size limit for the header, by default `MESSAGE_HEADER_SIZE` bytes).
   * `NON-BLOCKING`: returns a `JoinHandle` that can be awaited to wait for completion.
   */
  pub fn send<R>(
    self: &Arc<Self>,
    destination: LocationID,
    message_id: String,
    reader: R,
    header_data: Bytes,
    data_size: usize
  ) -> JoinHandle<()> where R: AsyncReadExt + Unpin + Send + 'static {
    let orchestra = self.clone();

    tokio::spawn(async move {
      orchestra.blocking_send(destination, message_id, reader, header_data, data_size, orchestra.location.clone()).await;
    })
  }

  /**
   * Reads the data in the reader `R` and sends it to the destination.
   * `header_data` is a byte array that can be used to send additional data with the message header.
    (note that there is a maximum size limit for the header, by default `MESSAGE_HEADER_SIZE` bytes).
   * `NON-BLOCKING`: adds a task to the `JoinSet` and returns the updated `JoinSet`.
   */
  pub fn send_joinset<R>(
    self: &Arc<Self>,
    destination: LocationID,
    location_id: String,
    reader: R,
    header_data: Bytes,
    data_size: usize,
    mut join_set: JoinSet<()>,
  ) -> JoinSet<()> where R: AsyncReadExt + Unpin + Send + 'static {
    let orchestra = self.clone();

    join_set.spawn(async move {
      orchestra.blocking_send(destination, location_id, reader, header_data, data_size, orchestra.location).await;
    });

    join_set
  }
}