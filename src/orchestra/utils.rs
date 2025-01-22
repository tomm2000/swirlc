use std::{collections::HashMap, path::PathBuf, process::{ExitStatus, Output, Stdio}};

use tokio::process::Child;

use super::LocationInfo;

pub fn data_size(size: usize) -> String {
  let kb = 1024;
  let mb = kb * 1024;
  let gb = mb * 1024;
  let tb = gb * 1024;

  if size < kb {
    format!("{} B", size)
  } else if size < mb {
    format!("{:.2} KB", size as f64 / kb as f64)
  } else if size < gb {
    format!("{:.2} MB", size as f64 / mb as f64)
  } else if size < tb {
    format!("{:.2} GB", size as f64 / gb as f64)
  } else {
    format!("{:.2} TB", size as f64 / tb as f64)
  }
}

pub fn debug_prelude<Loc>(location: &Loc, step_name: Option<&String>) -> String where Loc: std::fmt::Debug {
  // [HH:MM:SS] [location] [step_name] >>>
  let time = chrono::Local::now().format("%H:%M:%S").to_string();
  let location = format!("{:?}", location);
  let step_name = match step_name {
    Some(step_name) => format!(" [{}]", step_name),
    None => "".to_string(),
  };

  format!("[{}] [{}]{} >>> ", time, location, step_name)
}

pub fn addresses_from_config_file(file_path: &str) -> HashMap<String, LocationInfo> {
  let mut location_map = HashMap::new();

  let file = std::fs::read_to_string(file_path).unwrap();

  for line in file.lines() {
    let parts: Vec<&str> = line.split(',').collect();

    let location_info = LocationInfo {
      address: parts[2].to_string(),
      machine: parts[1].to_string(),
    };

    location_map.insert(parts[0].to_string(), location_info);
  }

  location_map
}

pub async fn execute_command_output(
  command: &String,
  arguments: &Vec<String>,
  workdir: &PathBuf
) -> Output {
  let child: Child;

  #[cfg(target_os = "linux")] {
  child = tokio::process::Command::new("sh")
    .arg("-c")
    .arg(format!("{} {}", command, arguments.join(" ")))
    .current_dir(workdir)
    .stdout(std::process::Stdio::piped())
    .spawn()
    .expect(format!("failed to spawn command: {:?}", command).as_str());
  }

  #[cfg(target_os = "windows")] {
  child = tokio::process::Command::new("powershell.exe")
    .arg("-Command")
    .arg(format!("{} {}", command, arguments.join(" ")))
    .current_dir(workdir)
    .stdout(std::process::Stdio::piped())
    .spawn()
    .expect(format!("failed to spawn command: {:?}", command).as_str());
  }

  return child
    .wait_with_output()
    .await
    .expect("failed to wait with output");
}

pub async fn execute_command(
  command: &String,
  arguments: &Vec<String>,
  workdir: &PathBuf
) -> ExitStatus {
  let mut child: Child;

  #[cfg(target_os = "linux")] {
  child = tokio::process::Command::new("sh")
    .arg("-c")
    .arg(format!("{} {}", command, arguments.join(" ")))
    .current_dir(workdir)
    .stdout(std::process::Stdio::null())
    .spawn()
    .expect(format!("failed to spawn command: {:?}", command).as_str());
  }

  #[cfg(target_os = "windows")] {
  child = tokio::process::Command::new("powershell.exe")
    .arg("-Command")
    .arg(format!("{} {}", command, arguments.join(" ")))
    .current_dir(workdir)
    .stdout(std::process::Stdio::null())
    .spawn()
    .expect(format!("failed to spawn command: {:?}", command).as_str());
  }

  return child
    .wait()
    .await
    .expect("failed to wait with output");
}