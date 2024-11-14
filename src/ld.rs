use std::{path::PathBuf, sync::Arc};

// =========================== PRELUDE ===========================
use crate::{amdahline::Amdahline, comm::{self, Communicator, PortData, StepOutput}, config::{LocationID, PortID, ADDRESSES}};

// =========================== MAIN FUNCTION ===========================
pub async fn run() {
  println!("Running LD");

  let workdir = PathBuf::from("workdir\\ld");
  let _ = std::fs::remove_dir_all(&workdir);
  
  let amdahline = Arc::new(Amdahline::new("ld.txt".to_string()));

  let communicator = Communicator::new(
    LocationID::LD,
    workdir,
    amdahline.clone(),
  ).await;

  amdahline.register_executor("LD".to_string());

  let communicator = Arc::new(communicator);

  let communicator_clone = communicator.clone();
  let exec = std::thread::spawn(move || async move {
    comm::exec(
      communicator_clone, // communicator
      "s1".to_string(), // name
      "individuals_merge".to_string(), // display name
      vec![],
      Some(PortID::P1), // output port
      StepOutput::File("message.txt".to_string()), // output type
      "ls".to_string(), // command
      vec![ // arguments
        "> message.txt".into(),
      ]
    ).await;
  });

  exec.join();

  let send_handle = comm::send(communicator.clone(), PortID::P1, LocationID::L1).await;
  
  tokio::join!(send_handle);

  communicator.close_connections();

  amdahline.unregister_executor("LD".to_string());
}
// =====================================================================