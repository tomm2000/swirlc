use std::{path::PathBuf, sync::Arc};

use crate::{comm::{self, Communicator, StepOutput}, config::{LocationID, PortID, ADDRESSES}};

// =========================== PRELUDE ===========================

// =========================== MAIN FUNCTION ===========================
pub async fn run() {
  println!("Running L1");

  let workdir = PathBuf::from("workdir\\l1");
  let _ = std::fs::remove_dir_all(&workdir);

  let communicator = Arc::new(Communicator::new(
    LocationID::L1,
    workdir
  ));

  let receive_handle = comm::receive(communicator.clone(), PortID::P1, LocationID::LD).await;

  tokio::join!(receive_handle);

  communicator.close_connections();
}
// =====================================================================