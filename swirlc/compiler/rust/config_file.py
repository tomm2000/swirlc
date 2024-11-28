from typing import MutableSequence

from swirlc.core.entity import DistributedWorkflow, Location

def build_config_file(file, locations: MutableSequence[Location], workflow: DistributedWorkflow):       # create the config.rs file
    locations_str = "{"
    for location in locations:
        locations_str += f"\n  {location.name.upper()},"
    locations_str += "\n}"

    ports_str = "{"
    for port in workflow.ports.values():
        ports_str += f"\n  {port.name.upper()},"
    ports_str += "\n}"

    location_str = ""
    for location in locations:
        location_str += f"    \"{location.name}\" => LocationID::{location.name.upper()},\n"
        
    config_str = f"""
use std::collections::HashMap;

use serde::{{Deserialize, Serialize}};
use strum_macros::EnumIter;

#[derive(Eq, PartialEq, Hash, Debug, Clone, Copy, Serialize, Deserialize)]
pub enum LocationID { locations_str }

#[derive(Eq, PartialEq, Hash, Debug, Clone, Copy, EnumIter, Serialize, Deserialize)]
pub enum PortID {ports_str}

#[derive(Clone)]
pub struct Addresses {{
  location_map: HashMap<LocationID, String>,
}}

impl Addresses {{
  pub fn from_address_map_file(file: &str) -> Addresses {{
    let mut location_map = HashMap::new();
    let file = std::fs::read_to_string(file).unwrap();
    for line in file.lines() {{
      let parts: Vec<&str> = line.split(',').collect();
      let location = match parts[0] {{
        {location_str}
        _ => panic!("Invalid location ID"),
      }};
      location_map.insert(location, parts[1].to_string());
    }}
    Addresses {{ location_map: location_map }}
  }}

  pub fn get_address(&self, location: LocationID) -> &str {{
    self.location_map.get(&location).unwrap()
  }}
}}
        """

    with open(file, 'w') as f:
        f.write(config_str)