
pub mod comm;
pub mod config;
pub mod l1;
pub mod ld;
pub mod utils;
pub mod amdahline;

#[tokio::main]
pub async fn main() {
  // cargo run -- --loc=l1

  let args: Vec<String> = std::env::args().collect();
  let loc = args[1].clone();

  match loc.as_str() {
    "--loc=l1" => l1::run().await,
    "--loc=ld" => ld::run().await,
    _ => panic!("Invalid location")
  }
}