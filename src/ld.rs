use std::{path::PathBuf, sync::Arc};

// =========================== PRELUDE ===========================
use crate::{comm::{self, Communicator, DataType, StepOutput}, config::{LocationID, PortID, ADDRESSES}};

// =========================== MAIN FUNCTION ===========================
pub async fn run() {
  println!("Running LD");

  let workdir = PathBuf::from("workdir\\ld");
  let _ = std::fs::remove_dir_all(&workdir);

  let communicator = Communicator::new(
    LocationID::LD,
    workdir
  );

  let communicator = Arc::new(communicator);

  let f1 = |communicator: Arc<Communicator>| async move {
    comm::exec(
      communicator.clone(), // communicator
      "s1".to_string(), // name
      "individuals_merge".to_string(), // display name
      Some(PortID::P1), // output port
      StepOutput::File("message.txt".to_string()), // output type
      "cat".to_string(), // command
      vec![ // arguments
        "> message.txt".into(),
      ]
    ).await;

    let send_handle = comm::send(communicator.clone(), PortID::P1, LocationID::L1).await;
    
    tokio::join!(send_handle);
  };

  tokio::spawn(f1(communicator.clone()));

  communicator.close_connections();
}
// =====================================================================