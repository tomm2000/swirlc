import sys
from swirlc.core.entity import DistributedWorkflow, Location
from swirlc.version import VERSION


def start_location_file(file, location: Location, workflow: DistributedWorkflow):
    workdir = location.workdir

    try:
        workdir = workdir.replace("\\", "\\\\")
    except:
        workdir = "./"

    with open(file, "w") as f:
        f.write(
f"""// ===========================================
// This file was generated automatically using SWIRL v{VERSION},
// using command swirlc {' '.join(sys.argv[1:])}
// ===========================================

use std::{{collections::HashMap, sync::Arc}};
use tokio::task::JoinSet;
use crate::{{orchestra::LocationInfo, swirl::{{PortData, StepArgument, StepOutput, Swirl}}}};

pub async fn {location.name}(location: String, address_map: HashMap<String, LocationInfo>) {{
  println!("Running {{}}", location);

  let start = std::time::Instant::now();

  let swirl = Arc::new(Swirl::new(location.clone(), address_map, "/workdir/{location.name}".into()));
""")
        
def close_location_file(file, location: Location, workflow: DistributedWorkflow):
    with open(file, "a") as f:
        f.write(
f"""\n
  println!("{location.name} finished in {{:?}}", start.elapsed());
//  ===================== end of location {location.name} =====================
}}
""")