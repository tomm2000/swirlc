import sys
from typing import MutableSequence

from swirlc.core.entity import Location
from swirlc.version import VERSION


def build_run_script(file, locations: MutableSequence[Location]):
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

