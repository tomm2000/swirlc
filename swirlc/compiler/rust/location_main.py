import sys
from swirlc.core.entity import DistributedWorkflow, Location
from swirlc.version import VERSION


def start_location_file(file, location: Location, workflow: DistributedWorkflow):
    workdir = location.workdir

    workdir = workdir.replace("\\", "\\\\")

    with open(file, "w") as f:
        f.write(
f"""// ===========================================
// This file was generated automatically using SWIRL v{VERSION},
// using command swirlc {' '.join(sys.argv[1:])}
// ===========================================

use std::{{path::PathBuf, sync::Arc}};
use swirlc_rust::{{amdahline::Amdahline, comm::{{Communicator, PortData, StepOutput}}}};
use swirlc_rust::config::{{Addresses, LocationID, PortID}};

#[tokio::main]
pub async fn main() {{
  let start = std::time::Instant::now();
  let addresses = Addresses::from_address_map_file("location_map.txt");

  let amdahline = Arc::new(Amdahline::new("amdahline_{location.name}.txt".to_string()));
  amdahline.register_executor("{location.name.upper()}".to_string());

  let workdir = PathBuf::from("{workdir}");

  let communicator = Arc::new(Communicator::new(
    LocationID::{location.name.upper()},
    workdir,
    amdahline.clone(),
    addresses
  ).await);
""")
        
def close_location_file(file, location: Location, workflow: DistributedWorkflow):
    with open(file, "a") as f:
        f.write(
f"""\n
  communicator.close_connections();
  
  amdahline.unregister_executor("{location.name.upper()}".to_string());
  amdahline.close();
  
  println!("{location.name} finished in {{:?}}", start.elapsed());

//  ===================== end of location {location.name} =====================
}}
""")