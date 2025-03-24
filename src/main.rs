pub mod swirl;
pub mod locations;
pub mod orchestra;
pub mod amdahline;

use clap::Parser;
use tokio::{process::Child, task::JoinSet};

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
  // Location
  #[arg(short, long)]
  loc: String,
}

#[tokio::main]
async fn main() {
  let address_map = orchestra::utils::addresses_from_config_file("address_map.txt");
  let mut join_set = JoinSet::new();

  for (location, _) in address_map.clone() {
    match location.as_str() {
      "location0" => join_set.spawn(locations::location0::location0("location0".to_string(), address_map.clone())),
      _ => continue,
    };
  }

  join_set.join_all().await;
}