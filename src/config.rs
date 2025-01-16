use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

#[derive(Eq, PartialEq, Hash, Debug, Clone, Copy, Serialize, Deserialize)]
pub enum LocationID {
  L1,
  LD,
}

#[derive(Eq, PartialEq, Hash, Debug, Clone, Copy, EnumIter, Serialize, Deserialize)]
pub enum PortID {
  P1,
  P2
}

#[derive(Clone)]
pub struct Addresses {
  location_map: HashMap<LocationID, String>,
}

impl Addresses {
  pub fn from_address_map_file(file: &str) -> Addresses {
    let mut location_map = HashMap::new();
    let file = std::fs::read_to_string(file).unwrap();
    for line in file.lines() {
      let parts: Vec<&str> = line.split(',').collect();
      let location = match parts[0] {
        "l1" => LocationID::L1,
        "ld" => LocationID::LD,
        _ => panic!("Invalid location ID"),
      };
      location_map.insert(location, parts[1].to_string());
    }
    Addresses { location_map }
  }

  pub fn get_address(&self, location: LocationID) -> &str {
    self.location_map.get(&location).unwrap()
  }
}