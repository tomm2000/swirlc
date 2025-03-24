n_locations = int(input("Number of locations: "))
# n_locations = 10

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


# source.swirl ==================
for i in range(1, n_locations):
    send = f"send(d1->p{i},{locations[i]},{locations[0]})"
    source_file.write(f"""
<{locations[i]}, {{(p{i}, d1)}}, {send}> |""")


receives = [f"recv(p{i},{locations[i]},{locations[0]})" for i in range(1, n_locations)]

source_file.write(f"""
<{locations[0]}, {{}},
  (
    {" |\n    ".join(receives)}
  )
>
""")