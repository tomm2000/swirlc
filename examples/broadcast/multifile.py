n_locations = int(input("Number of locations: "))
# n_locations = 10
n_files = int(input("Number of files: "))

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


    
# dependencies ==================
deps = ""
for i in range(n_files):
    deps += f"""
  d{i}:
    type: file
    value: /data/file{i}.txt"""

config_file.write(f"\n\ndependencies:{deps}")

# ports inits ==================
port_init = []
for i in range(n_files):
    port_init.append(f"(p{i}, d{i})")
port_init = ", ".join(port_init)

main_loc = f"<{locations[0]}, {{{port_init}}}"

# sends ==================
sends = ""
for j in range(n_files):
    tmp = []
    for i in range(1, n_locations):
            tmp.append(f"send(d{j}->p{j},{locations[0]},{locations[i]})")
    tmp = " |\n    ".join(tmp)

    sends += f"""
  (
    {tmp}
  )
"""
    sends = sends.strip()
    if j != n_files - 1:
        sends += " |"

main_loc += f""",
  {sends}
> |\n"""

# receives ==================
for i in range(1, n_locations):
    receives = []
    for j in range(n_files):
        receives.append(f"recv(p{j},{locations[0]},{locations[i]})")
    receives = " |\n    ".join(receives)

    main_loc += f"""
<location{i}, {{}}, (
    {receives}
) >"""
    
    # if not last location add a pipe
    if i != n_locations - 1:
        main_loc += " |\n"
    

source_file.write(main_loc)