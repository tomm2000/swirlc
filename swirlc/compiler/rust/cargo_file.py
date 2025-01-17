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
serde = { version = "1.0.214", features = ["derive"] }
chrono = "0.4.39"
tokio = { version = "1.42.0", features = ["full"]}
bincode = "1.3.3"
strum = "0.26.3"
strum_macros = "0.26"
glob = "0.3.1"
uuid = { version = "1.11.0", features = ["v4"] }
''')

