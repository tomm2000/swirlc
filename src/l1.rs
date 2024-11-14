use std::{path::PathBuf, sync::Arc};

use crate::{amdahline::Amdahline, comm::{self, Communicator, StepOutput}, config::{LocationID, PortID, ADDRESSES}};

// =========================== PRELUDE ===========================

// =========================== MAIN FUNCTION ===========================
pub async fn run() {
  println!("Running L1");

  let start = std::time::Instant::now();

  let workdir = PathBuf::from("workdir\\l1");
  let _ = std::fs::remove_dir_all(&workdir);

  let amdahline = Arc::new(Amdahline::new("l1.txt".to_string()));

  let communicator = Arc::new(Communicator::new(
    LocationID::L1,
    workdir,
    amdahline.clone(),
  ).await);

  amdahline.register_executor("L1".to_string());

  let receive_handle = comm::receive(communicator.clone(), PortID::P1, LocationID::LD).await;

  tokio::join!(receive_handle);

  println!("L1 finished in {:?}", start.elapsed());

  communicator.close_connections();

  amdahline.unregister_executor("L1".to_string());

  amdahline.close();
}
// =====================================================================