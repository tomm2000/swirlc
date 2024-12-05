import sys
from typing import MutableSequence

from swirlc.core.entity import Location
from swirlc.version import VERSION


def build_run_script(file, locations: MutableSequence[Location], env: str, build_mode: str, output_dir: str):

  if env == "apptainer":
    nnodes_str = len(locations)
    locations_str = ' '.join([f'"{loc.name}"' for loc in locations])

    with open(file, 'w') as f:
      f.write(
  f'''#!/bin/bash

# This file was generated automatically using SWIRL v{VERSION},
# using command swirlc {' '.join(sys.argv[1:])}

#SBATCH --nodes={nnodes_str}
#SBATCH --cpus-per-task=36
#SBATCH --tasks-per-node=1
#SBATCH --partition=broadwell

# Activate Spack environment
spack env activate swirl

# Prepare locations and node mapping
locations=({locations_str})
nodes=$(scontrol show hostnames $SLURM_NODELIST)
i=0

# Clear or create location map file
> location_map.txt

# clear the workdir
rm -rf ~/.swirl/workdir/*

# Create location map
while read -r node; do
  echo "${{locations[$i]}},${{node}}:8080" >> location_map.txt
  echo "Assigned ${{locations[$i]}} to ${{node}}"
  ((i++))
done <<< "$nodes"

echo "Created location map"

# Loop through the location list and run using Apptainer
while read -r location; do
  loc=$(echo $location | cut -d',' -f1)
  node=$(echo $location | cut -d',' -f2 | cut -d':' -f1)
  
  srun --nodes 1 -w $node apptainer exec \\
    --bind ~/.swirl/outputs:/outputs \\
    --bind ~/.swirl/workdir:/workdir \\
    --bind ~/data:/data \\
    docker://mul8/1000genome-swirlc \\
    ./target/release/$loc &
    
done < location_map.txt

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