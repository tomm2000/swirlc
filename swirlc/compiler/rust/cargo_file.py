from swirlc.version import VERSION

def build_cargo_file(file):
    with open(file, 'w') as f:
        f.write(f'''
[package]
name = "swirlc-rust"
version = "{VERSION}"
edition = "2021"
''' + '''
[dependencies]
serde = { version = "1.0.217", features = ["derive"] }
chrono = "0.4"
tokio = { version = "1.43", features = ["full"]}
bincode = "1.3"
strum = "0.26.3"
strum_macros = "0.26"
glob = "0.3.2"
uuid = { version = "1.12.0", features = ["v4"] }
clap = { version = "4.5.21", features = ["derive"] }
bytes = "1.9.0"
''')

