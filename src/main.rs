pub mod comm;
pub mod config;
pub mod l1;
pub mod ld;
pub mod utils;
pub mod amdahline;

use clap::Parser;

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
  // Location
  #[arg(short, long)]
  loc: String,
}

#[tokio::main]
pub async fn main() {
  let args = Args::parse();

  let loc = args.loc;

  match loc.as_str() {
    "l1" => l1::run().await,
    "ld" => ld::run().await,
    _ => panic!("Invalid location")
  }
}