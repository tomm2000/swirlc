use chrono::Local;
use std::{env, time::Duration};
use systemstat::{Platform, System};
use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  net::{TcpListener, TcpStream},
};

pub mod config_schema;

#[tokio::main]
async fn main() {
  let mut port = String::from("6379");
  if let Some(_) = env::args().find(|f| f.starts_with("-port")) {
    port = env::args().nth(2).unwrap_or(String::from("6379"));
  }

  println!("Application started");
  
  if let Some(_) = env::args().find(|f| f.starts_with("-server")) {
    server(port).await;
  } else {
    client(port).await;
  }
}

async fn server(port: String) {
  let listener = TcpListener::bind(format!("0.0.0.0:{}", port)).await.unwrap();
  listener.set_ttl(255).expect("not set TTL");

  println!("Waiting to clients... [Port: {}]", port);

  loop {
    match listener.accept().await {
      Ok((socket, _)) => {
        tokio::spawn(async move {
          process(socket).await;
        });
      }
      Err(e) => println!("Error in acception: {}", e),
    }
  }
}

async fn client(port: String) {
  let mut socket = TcpListener::bind(format!("0.0.0.0:{}", port)).await.unwrap();
  socket.set_ttl(255).expect("not set TTL");

  println!("Connecting to server...");

  let mut stream = TcpStream::connect("127.0.0.1:1234").await;

  if let Err(e) = stream {
    println!("Error in connection: {}", e);
    return;
  }

  let mut stream = stream.unwrap();

  println!("Connected to server");

  loop {
    let mut buffer = [0; 1024];
    match stream.read(&mut buffer).await {
      Ok(n) => {
        let data = &buffer[..n];
        let packet_type = data[0];
        let size = (data[1] as usize) + ((data[2] as usize) << 8);
        let data = &data[3..size + 3];
        let data = String::from_utf8_lossy(data);

        match packet_type {
          1 => println!("Time: {}", data),
          2 => println!("Memory: {}", data),
          3 => println!("CPU: {}", data),
          _ => println!("Unknown packet type: {}", packet_type),
        }
      }
      Err(e) => {
        println!("Error in reading: {}", e);
        return;
      }
    }
  }
}



async fn process(mut socket: TcpStream) {
  let sys = System::new();
  println!("Connected this client, Remote Addr: {:?}", socket.peer_addr().unwrap());
  socket.set_nodelay(true).unwrap();

  loop {
    let local = Local::now();
    let time = local.format("%H%M%S").to_string();
    let mut packet: Vec<u8> = vec![];

    add_packet(&mut packet, 1, &time); // Add time info

    if let Ok(mem) = sys.memory() {
      add_packet(&mut packet, 2, &mem.total.as_u64().to_string()); // Add memory info
    }
    if let Ok(cpu) = sys.cpu_load_aggregate() {
      std::thread::sleep(Duration::from_secs(1));
      let cpu = cpu.done().unwrap();
      add_packet(&mut packet, 3, &(cpu.user * 100.0).to_string()); // Add cpu info
    } else {
      std::thread::sleep(Duration::from_secs(1));
    }

    if let Err(e) = socket.write_all(&packet).await { // Send it
      println!("During send, socket has given error: {}", e);
      return;
    }
  }
}

fn add_packet(packet: &mut Vec<u8>, packet_type: u8, data: &str) {
  let data = data.as_bytes();
  let size = data.len();

  let low = (size & 0x00FF) as u8;
  let hi = (size >> 8_u8) as u8;

  packet.push(packet_type);
  packet.push(low);
  packet.push(hi);
  packet.extend_from_slice(&data);
}