pub mod comm;
pub mod config;
pub mod l1;
pub mod ld;

#[tokio::main]
pub async fn main() {
  // cargo run -- --loc=l1

  // let args: Vec<String> = std::env::args().collect();
  // let loc = args[1].clone();

  // match loc.as_str() {
  //   "--loc=l1" => l1::run().await,
  //   "--loc=ld" => ld::run().await,
  //   _ => panic!("Invalid location")
  // }


  let a = test_async(1);
  let b = test_async(2);

  // a.await.unwrap();
  // b.await.unwrap();

  tokio::join!(a, b);
}


// pub fn test_async(id: i32) -> tokio::task::JoinHandle<()> {
//   tokio::spawn(async move {
//     tokio::time::sleep(std::time::Duration::from_secs(2)).await;
//     println!("Hello, world! {}", id);
//   })
// }

pub async fn test_async(id: i32) {
  tokio::time::sleep(std::time::Duration::from_secs(2)).await;
  println!("Hello, world! {}", id);
}