use std::{ops::Deref, path::PathBuf};

use crate::{orchestra::utils::{self, debug_prelude, execute_command}, swirl::PortData};

use super::{PortID, StepArgument, StepOutput, Swirl};


impl Swirl {
  pub async fn exec(
    &self,
    step_name: String,
    step_display_name: String,
    input_ports: Vec<PortID>,
    output_port: Option<PortID>,
    output_type: StepOutput,
    cmd: String,
    args: Vec<StepArgument>,
  ) {
    let mut step_workdir = self.workdir.join(format!("step_{}", step_name));
  
    let step_workdir_str = step_workdir.to_str().expect("failed to convert to string");
    let step_workdir_str = format!("failed to convert to string: {:?}", step_workdir_str);
    std::fs::create_dir_all(&step_workdir).expect(&step_workdir_str);
  
    step_workdir = self.workdir.join(format!("step_{}", step_name));
    step_workdir = step_workdir
      .canonicalize()
      .expect(format!("failed to canonicalize {:?}", step_workdir).as_str());
  
    // loop over the ports
    for input_port in input_ports {
      let port = self.ports.get(&input_port).expect("port not found");
  
      port.wait_for_data().await;
  
      let data = port.value.read().await;
      let data = data.deref();
  
      match data {
        PortData::File(path) => {
          // link the file to the step workdir
          let file_path = PathBuf::from(path);
          let file_name = file_path
            .file_name()
            .expect("failed to get file name")
            .to_str()
            .expect("failed to convert to string")
            .to_string();
  
          let new_path = step_workdir.join(&file_name);
          
  
          // create symlink
          #[cfg(unix)]
          {
            std::os::unix::fs::symlink(&file_path, &new_path).expect("failed to create symlink");
          }
        }
        PortData::Empty => {
          panic!("empty data");
        }
        _ => {}
      }
    }
  
    //======================== Build arguments ========================
    let mut arguments: Vec<String> = vec![];
  
    for arg in args {
      match arg {
        StepArgument::String(value) => {
          arguments.push(value);
        }
        StepArgument::Port(port_id) => {
          let port = self.ports.get(&port_id).expect("port not found");
  
          port.wait_for_data().await;
  
          let data = port.value.read().await;
          let data = data.deref();
  
          match data {
            PortData::File(path) => {
              // if the argument is a file, the file should be already linked to the step workdir
              let filename = PathBuf::from(path)
                .file_name()
                .expect("failed to get file name")
                .to_str()
                .expect("failed to convert to string")
                .to_string();
              arguments.push(filename);
            }
            PortData::String(value) => {
              arguments.push(value.clone());
            }
            PortData::Int(value) => {
              arguments.push(value.to_string());
            }
            PortData::Bool(value) => {
              arguments.push(value.to_string());
            }
            PortData::Empty => {
              panic!("empty data");
            }
          }
        }
      }
    }
  
    //======================== Execute Command ========================
    println!(
      "{} Running command: '{} {}'",
      debug_prelude(&self.orchestra.self_name(), Some(&step_name)),
      cmd,
      arguments.join(" ")
    );
  
    let (output, status) = match output_type {
      StepOutput::Stdout => {
        let output = utils::execute_command_output(&cmd, &arguments, &step_workdir).await;
        let status = output.status;

        println!(
          "{} Completed step: {} with status: {}",
          debug_prelude(&self.orchestra.self_name(), Some(&step_name)),
          step_display_name,
          status,
        );

        (Some(output), status)
      }
      _ => {
        let status = utils::execute_command(&cmd, &arguments, &step_workdir).await;

        println!(
          "{} Completed step: {} with status: {}",
          debug_prelude(&self.orchestra.self_name(), Some(&step_name)),
          step_display_name,
          status,
        );
  
        (None, status)
      }
    };
  
    if !status.success() {
      panic!("Command failed with status: {}", status);
    }
  
    if output_port.is_some() {
      let port = self
      .ports
      .get(&output_port.unwrap())
      .expect("port not found");
  
      match output_type {
        StepOutput::File(path_regex) => {
          let path_regex = path_regex.replace("/", "\\");
  
          let path_regex = step_workdir.join(path_regex);
  
          let path_regex = path_regex
            .to_str()
            .expect("failed to convert to string")
            .to_string();
  
          let res = glob::glob(path_regex.as_str()).expect("failed to glob");
          let res = res
            .collect::<Result<Vec<_>, _>>()
            .expect("failed to collect");
  
          if res.len() == 0 {
            let available_files = std::fs::read_dir(&step_workdir)
              .expect("failed to read dir")
              .map(|res| res.unwrap().path())
              .collect::<Vec<_>>();
            panic!(
              "No files found for regex: {}, available files: {:?}",
              path_regex, available_files
            );
          }
  
          if res.len() > 1 {
            panic!("Multiple files found for regex: {}", path_regex);
          }
  
          let path = res[0]
            .to_str()
            .expect("failed to convert to string")
            .to_string();
  
          port.set(PortData::File(path)).await;
          port.port_ready.notify_waiters();
        }
        StepOutput::Stdout => {
          let stdout = String::from_utf8(output.expect("failed to get output").stdout)
            .expect("failed to convert output to string");
  
          port.set(PortData::String(stdout)).await;
          port.port_ready.notify_waiters();
        }
        StepOutput::None => {
          port.set(PortData::Empty).await;
          port.port_ready.notify_waiters();
        }
      }
    }
  
    // self.amdahline.end_task(format!("{:?}", self.location), t);
  }
}
