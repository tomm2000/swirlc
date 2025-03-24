import sys
import math
from typing import MutableSequence

from swirlc.core.entity import Location
from swirlc.version import VERSION

MAX_NODES = 20
CPUS_PER_NODE = 36

def build_run_script(file, locations: MutableSequence[Location], env: str, build_mode: str, output_dir: str):

  if env == "apptainer":
    n_locations = len(locations)
    n_nodes = min(n_locations, MAX_NODES)
    n_tasks_per_node = math.ceil(n_locations / n_nodes)
    cpus_per_task = CPUS_PER_NODE // n_tasks_per_node

    locations_str = ' '.join([f'"{loc.name}"' for loc in locations])

    with open(file, 'w') as f:
      f.write(
  f'''#!/bin/bash

# This file was generated automatically using SWIRL v{VERSION},
# using command swirlc {' '.join(sys.argv[1:])}

#SBATCH --nodes={n_nodes}
#SBATCH --cpus-per-task={cpus_per_task}
#SBATCH --tasks-per-node={n_tasks_per_node}
#SBATCH --partition=broadwell

# Activate Spack environment
spack env activate swirl

# Prepare locations and node mapping
locations=({locations_str})
nodes=$(scontrol show hostnames $SLURM_NODELIST)
nodes=($nodes)

# Clear or create location map file
> address_map.txt

# clear the workdir
rm -rf ~/.swirl/workdir/*

num_nodes=$SLURM_NNODES
num_locations=${{#locations[@]}}

echo "Number of nodes: $num_nodes"
echo "Number of locations: $num_locations"

# Round-robin location assignment
for ((i=0; i < num_locations; i++)); do
  # Use modulo to cycle through nodes
  node_index=$((i % num_nodes))

  # create the port, starting from 8080 and incrementing by 1 every $num_nodes locations
  port=$((8080 + i / num_nodes))
  
  # Assign location to node and write to location map
  echo "${{locations[$i]}},${{nodes[$node_index]}},${{nodes[$node_index]}}:$port" >> address_map.txt
  echo "Assigned ${{locations[$i]}} to ${{nodes[$node_index]}}"
done

echo "Created location map"

# Loop through the locations and run using Apptainer
for ((i=0; i < num_locations; i++)); do
  node_index=$((i % num_nodes))

  loc=${{locations[$i]}}
  node=${{nodes[$node_index]}}

  echo "Running $loc on $node"

  srun --ntasks 1 --nodes 1 -w $node apptainer exec \\
    --bind ~/.swirl/outputs:/outputs \\
    --bind ~/.swirl/workdir:/workdir \\
    --bind ~/data:/data \\
    docker://mul8/1000genome-swirlc \\
    ./swirlc-rust --loc=$loc &
done

wait
      ''')
  elif env == "docker":



    with open(file, 'w') as f:
      copy_commands_str = ""
      for location in locations:
        file = f"./build/target/{build_mode}/{location.name}"
        command = location.get_copy_command(file, f"{output_dir}/{location.hostname}:{location.workdir}")

        if command:
          copy_commands_str += f"{command} &\n"

      execution_commands_str = ""
      for location in locations:
        command = location.get_command(f"./{location.name}")

        if command:
          execution_commands_str += f"{command} &\n"

      f.write(
  f'''#!/bin/bash

# This file was generated automatically using SWIRL v{VERSION},
# using command swirlc {' '.join(sys.argv[1:])}

trap "echo Force termination; pkill -P $$" INT

# Start workflow execution

{copy_commands_str}

wait

{execution_commands_str}

wait
''')

  else:
    raise Exception(f"Environment `{env}` not supported")