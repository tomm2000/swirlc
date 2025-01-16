// use std::{path::PathBuf, sync::Arc};

// use crate::{amdahline::Amdahline, comm::{self, Communicator}, config::{Addresses, LocationID, PortID}};

// // =========================== PRELUDE ===========================

// // =========================== MAIN FUNCTION ===========================
// pub async fn run() {
//   println!("Running L1");

  
//   // spawn "ls -l /bin"
//   let output = std::process::Command::new("sh")
//     .arg("-c")
//     .arg("ls -l /bin")
//     .output()
//     .expect("failed to execute process");

//   // print the output
//   println!("status: {}", output.status);
//   println!("stdout: {}", String::from_utf8_lossy(&output.stdout));

//   let start = std::time::Instant::now();

//   let workdir = PathBuf::from("workdir/l1");
//   let _ = std::fs::remove_dir_all(&workdir);

//   let addresses = Addresses::from_address_map_file("location_map.txt");

//   let amdahline = Arc::new(Amdahline::new("l1.txt".to_string()));

//   let communicator = Communicator::new(
//     LocationID::L1,
//     workdir,
//     amdahline.clone(),
//     addresses
//   ).await;
  

//   amdahline.register_executor("L1".to_string());

//   let receive_handle = communicator.receive(PortID::P1, LocationID::LD).await;

//   tokio::join!(receive_handle);

//   let group_0 = tokio::spawn({ let communicator = communicator.clone(); async move {
//     communicator.send(PortID::P2, LocationID::LD).await;
//   }});

//   tokio::join!(group_0);

//   println!("L1 finished in {:?}", start.elapsed());

//   communicator.close_connections();

//   amdahline.unregister_executor("L1".to_string());

//   amdahline.close();
// }
// // =====================================================================