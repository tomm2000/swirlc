from typing import MutableSequence

from swirlc.core.entity import DistributedWorkflow, Location


# pub const PORTS: &[&str] = &[
#   "p1",
#   "p2",
# ];


def build_config_file(file, locations: MutableSequence[Location], workflow: DistributedWorkflow):       # create the config.rs file
    ports = workflow.ports
    ports_str = ',\n'.join([f'  "{port}"' for port in ports])
    config_str = f'pub const PORTS: &[&str] = &[\n{ports_str}\n];\n'

    with open(file, 'w') as f:
        f.write(config_str)