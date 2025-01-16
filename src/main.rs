pub mod swirl;
pub mod config;
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
    println!("{} ", location);
    match location.as_str() {
      "location0" => join_set.spawn(locations::location0::location0("location0".to_string(), address_map.clone())),
      "location1" => join_set.spawn(locations::location1::location1("location1".to_string(), address_map.clone())),
      "location2" => join_set.spawn(locations::location2::location2("location2".to_string(), address_map.clone())),
      _ => panic!("Invalid location: {}", location)
    };
  }

  join_set.join_all().await;
}