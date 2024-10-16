use std::collections::HashMap;

use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

#[derive(Eq, PartialEq, Hash, Debug, Clone, Copy, Serialize, Deserialize)]
pub enum LocationID {
  L1,
  LD,
}

#[derive(Eq, PartialEq, Hash, Debug, Clone, Copy, EnumIter)]
pub enum PortID {
  P1,
  P2
}
// build address map with lazy_static
lazy_static! {
  pub static ref ADDRESSES: HashMap<LocationID, String> = {
    let mut m = HashMap::new();
    m.insert(LocationID::LD, "127.0.0.1:8082".to_string());
    m.insert(LocationID::L1, "127.0.0.1:8081".to_string());
    m
  };
}