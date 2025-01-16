use std::{path::PathBuf, sync::Arc};

// =========================== PRELUDE ===========================
use crate::{amdahline::Amdahline, comm::{self, Communicator, PortData, StepOutput}, config::{Addresses, LocationID, PortID}};

// =========================== MAIN FUNCTION ===========================
pub async fn run() {
  println!("Running LD");

  let start = std::time::Instant::now();

  let workdir = PathBuf::from("workdir/ld");
  let _ = std::fs::remove_dir_all(&workdir);

  let addresses = Addresses::from_address_map_file("location_map.txt");
  
  let amdahline = Arc::new(Amdahline::new("ld.txt".to_string()));

  let communicator = Arc::new(Communicator::new(
    LocationID::LD,
    workdir,
    amdahline.clone(),
    addresses
  ).await);

  amdahline.register_executor("LD".to_string());

  println!("starting step s1");

  communicator.exec(
    "s1".to_string(), // name
    "step s1".to_string(), // display name
    vec![],
    Some(PortID::P1), // output port
    StepOutput::File("message.txt".to_string()), // output type
    "ls".to_string(), // command
    vec![ // arguments
      "> message.txt".into(),
    ]
  ).await;

  println!("waiting for step s1 to finish");

  let send_handle = communicator.send(PortID::P1, LocationID::L1).await;
  
  tokio::join!(send_handle);

  println!("LD finished in {:?}", start.elapsed());

  communicator.close_connections();

  amdahline.unregister_executor("LD".to_string());
}
// =====================================================================