import os
import shutil

from swirlc.core.entity import DistributedWorkflow, Location


def build_rust_lib(output_dir):
    current_folder = os.path.dirname(__file__)
    current_folder += "/src"

    destination_folder = output_dir + "/src"

    shutil.copytree(current_folder, destination_folder, dirs_exist_ok=True)  # Ignore if symlink target doesn't exist

def build_main_file(output_dir, locations: list[Location]):
    location_spawns = ""

    for location in locations:
        location_spawns += f"""
\t\t"{location.name}" => join_set.spawn(locations::{location.name}::{location.name}("{location.name}".to_string(), address_map.clone())),"""


    with open(output_dir + "/src/main.rs", "w") as f:
        f.write(
f"""
pub mod swirl;
pub mod locations;
pub mod orchestra;
pub mod amdahline;

use clap::Parser;
use tokio::{{process::Child, task::JoinSet}};

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {{
    // Location
    #[arg(short, long)]
    loc: String,
}}

#[tokio::main]
async fn main() {{
  let address_map = orchestra::utils::addresses_from_config_file("address_map.txt");
  let mut join_set: JoinSet<()> = JoinSet::new();

  let args = Args::parse();
  let location = args.loc;

  match location.as_str() {{{location_spawns}
    _ => panic!("Invalid location: {{}}", location)
  }};

  join_set.join_all().await;
}}
""")

def build_locations_module(output_dir, locations: list[Location]):
    locations_mod = ""

    for location in locations:
        locations_mod += f"pub mod {location.name};\n"

    with open(output_dir + "/src/locations/mod.rs", "w") as f:
        f.write(locations_mod)

# pub mod location0;
# pub mod location1;
# pub mod location2;