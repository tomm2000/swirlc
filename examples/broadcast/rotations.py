n_locations = int(input("Number of locations: "))
# n_locations = 10
n_rotations = int(input("Number of rotations: "))

config_file = open("config.yml", "w")
source_file = open("source.swirl", "w")

locations = [f"location{i}" for i in range(n_locations)]

# config.yml ==================
config_file.write("version: v1.0\n\n")
config_file.write("locations:")

for i in range(n_locations):
    config_file.write(f"""
  {locations[i]}:
    hostname: 127.0.0.1
    port: {8080 + i}
    workdir: /workdir""")

config_file.write(f"""\n
dependencies:
  d1:
    type: file
    value: /data/message.txt
    """) 

main_loc = f"<{locations[0]}, {{(p1, d1)}},"

# source.swirl ==================
for i in range(n_rotations):
    sends = [f"send(d1->p1,{locations[0]},{locations[i]})" for i in range(1, n_locations)]

    main_loc += f"""
  (
    {" |\n    ".join(sends)}
  )."""
    
    receives = [f"recv(p1,{locations[0]},{locations[i]})" for i in range(1, n_locations)]
    main_loc += f"""
  (
    {" |\n    ".join(receives)}
  )"""

    # add a . if it's not the last location
    if i < n_rotations - 1:
        main_loc += "."
    else:
        main_loc += "\n> |"

main_loc += "\n\n"

source_file.write(main_loc)

for i in range(1, n_locations):
    loc = f"<{locations[i]}, {{}},"
    for j in range(n_rotations):
        loc += f"\n  recv(p1,{locations[0]},{locations[i]})."
        loc += f"\n  send(d1->p1,{locations[0]},{locations[i]})"
        
        if j < n_rotations - 1:
            loc += "."
        else:
            # dont add | if it's the last location
            loc += f"\n> |" if i < n_locations - 1 else "\n>"

    loc += "\n\n"
    source_file.write(loc)


        
