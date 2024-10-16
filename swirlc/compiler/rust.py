from __future__ import annotations

import os
import stat
import sys
from pathlib import Path
from typing import MutableMapping, MutableSequence, TextIO
import shutil
from threading import Thread
import time

from black import WriteBack

from swirlc.core.compiler import BaseCompiler
from swirlc.core.entity import Location, Step, Port, Workflow, DistributedWorkflow, Data
from swirlc.log_handler import logger
from swirlc.version import VERSION

# "release" or "debug"
BUILD_MODE = "debug"

cargo_toml = """
[package]
name = "[LOCATION_NAME]"
version = "0.1.0"
edition = "2021"

# See more keys and their definitions at https://doc.rust-lang.org/cargo/reference/manifest.html

[dependencies]
serde = { version = "1.0.210", features = ["derive"] }
chrono = "0.4"
tokio = { version = "1.40.0", features = ["full"] }
systemstat = "0.2.3"
serde_yml = "0.0.12"
lazy_static = "1.5.0"
bincode = "1.3.3"
strum = "0.26.3"
strum_macros = "0.26"
pathdiff = "0.2.2"
glob = "0.3.1"
"""

bash_header = f"""#!/bin/sh

# This file was generated automatically using SWIRL v{VERSION},
# using command swirlc {' '.join(sys.argv[1:])}
"""

rust_main_start = """
use std::{path::PathBuf, sync::Arc};
use comm::{Communicator, DataType, StepOutput};
use config::{LocationID, PortID, ADDRESSES};

pub mod comm;
pub mod config;

#[tokio::main]
pub async fn main() {
"""

rust_main_end = """
  communicator.close_connections();
}
"""


class ThreadStack:
    def __init__(self):
        self.stack: MutableSequence[set[str]] = [set()]
        self.counter = 0

    def add_group(self) -> None:
        self.stack.append(set())

    def add_thread(self) -> str:
        name = f"t{self.counter}"
        self.counter += 1
        self.stack[-1].add(name)
        return name

    def delete_group(self) -> set[str]:
        return self.stack.pop()

    def get_group(self) -> set[str]:
        return self.stack[-1]


class RustTarget(BaseCompiler):
    def __init__(self):
        super().__init__()
        self.current_location: Location | None = None
        self.functions = []
        self.function_counter = 0
        self.parallel_step_counter = 0
        # If `parathetized` attribute is to True it means that an open bracket has been encountered
        # but not yet its corresponding closed bracket
        self.parathetized = False
        self.programs: MutableMapping[str, TextIO] = {}
        self.workflow: DistributedWorkflow | None = None
        self.thread_stacks: MutableMapping[str, ThreadStack] = {}
        self.active_locations: MutableSequence[Location] = []

    def _get_indentation(self):
        return " " * 4 if self.parallel_step_counter > 0 else ""

    def _get_thread(self, location: str) -> str:
        return self.thread_stacks.setdefault(location, ThreadStack()).add_thread()
    
    def begin_workflow(self, workflow: Workflow) -> None:
        # clear the build directory recursively
        shutil.rmtree("build", ignore_errors=True)
        
        self.workflow = workflow

    def end_workflow(self) -> None:
        # build the workspace cargo.toml
        with open(f"build/Cargo.toml", "w") as f:
            f.write(
f"""
[workspace]
members = [{', '.join([f'"{location.name}"' for location in self.active_locations])}]
resolver = "2"
""" 
            )

        # build the run.sh script in the build directory
        with open(f"build/run.sh", "w") as f:
            f.write(bash_header)
            
            f.write(
f"""
trap "echo Force termination; pkill -P $$" INT

# Start workflow execution
"""
            )

            for location in self.active_locations:
                f.write(
f"""
./target/{BUILD_MODE}/{location.name}.exe &"""
                )

            f.write(
f"""
wait
echo "Workflow execution terminated"
"""
            )
            
        # format the rust code
        os.system(f"cd build && cargo fmt")

        # fix the code if on release mode
        if BUILD_MODE == "release":
            os.system(f"cd build && cargo fix --allow-no-vcs --workspace")

        # compile the rust code
        release = "--release" if BUILD_MODE == "release" else ""
        # os.system(f"cd build && cargo build {release}")

    # DONE
    def begin_location(self, location: Location) -> None:
      build_path = f"build/{location.name}/"
      self.current_location = location
      self.active_locations.append(location)

      # copy the "rust_base" directory to the build path
      shutil.copytree("rust_base", build_path)

      # create the cargo.toml file
      with open(f"{build_path}Cargo.toml", "w") as f:
        f.write(cargo_toml.replace("[LOCATION_NAME]", location.name))

      # create main.rs file
      with open(f"{build_path}src/main.rs", "w") as f:
        f.write(rust_main_start)

        f.write(f"""
  let workdir = PathBuf::from("workdir\\\\{location.name}");
  """
        )

        f.write(f"""
  let communicator = Arc::new(Communicator::new(
    LocationID::{location.name.upper()},
    workdir
  ));
  """
        );

      # save main.rs file
      self.programs[self.current_location.name] = open(
          f"{build_path}src/main.rs", "a" 
      )

    def end_location(self) -> None:
        if self.thread_stacks[self.current_location.name].get_group():
            self.programs[self.current_location.name].write(
                f"""
    tokio::join!({', '.join(self.thread_stacks[self.current_location.name].delete_group())}); // end location
    """
            )

        # end the main.rs file
        self.programs[self.current_location.name].write(rust_main_end)

        locations = "{"
        for location in self.workflow.locations.values():
            locations += f"\n  {location.name.upper()},"
        locations += "\n}"

        ports = "{"
        for port in self.workflow.ports.values():
            ports += f"\n  {port.name.upper()},"
        ports += "\n}"

        addresses = ""
        for location in self.workflow.locations.values():
            addresses += f"    m.insert(LocationID::{location.name.upper()}, \"{location.hostname}:{location.port}\".to_string());\n"
        

        config_string = f"""
use std::collections::HashMap;

use lazy_static::lazy_static;
use serde::{{Deserialize, Serialize}};
use strum_macros::EnumIter;

#[derive(Eq, PartialEq, Hash, Debug, Clone, Copy, Serialize, Deserialize)]
pub enum LocationID { locations }

#[derive(Eq, PartialEq, Hash, Debug, Clone, Copy, EnumIter)]
pub enum PortID {ports}

lazy_static! {{
  pub static ref ADDRESSES: HashMap<LocationID, String> = {{
    let mut m = HashMap::new();
{addresses}\t\tm
  }};
}}
        """

        with open(f"build/{self.current_location.name}/src/config.rs", "w") as f:
            f.write(config_string)

        # close the main.rs file
        self.programs[self.current_location.name].close()

    # UNTESTED
    def begin_dataset(
        self,
        dataset: MutableSequence[tuple[str, Data]],
    ):
        for port_name, data in dataset:
            self.current_location.data[data.name] = data
            if data.type == "file":
                self.programs[self.current_location.name].write(f"""
  communicator.init_port(PortID::{port_name.upper()}, DataType::File("{data.value}".to_string())).await;"""
                )

            elif data.type == "string":
                self.programs[self.current_location.name].write(f"""
  communicator.init_port(PortID::{port_name.upper()}, DataType::String("{data.value}".to_string())).await;
  """
                )

            elif data.type == "int":
                self.programs[self.current_location.name].write(f"""
  communicator.init_port(PortID::{port_name.upper()}, DataType::Int({data.value})).await;
  """
                )

            elif data.type == "bool":
                self.programs[self.current_location.name].write(f"""
  communicator.init_port(PortID::{port_name.upper()}, DataType::Bool({data.value})).await;
  """
                )

            else:
                raise ValueError(f"Unsupported data type: {data.type}")



    def choice(self):
        raise NotImplementedError("Choice is not implemented yet")

    def exec(
        self,
        step: Step,
        flow: tuple[set[tuple[str, str]], set[tuple[str, str]]],
        mapping: set[str],
    ):
        outputs = flow[1]
        output_port_name = next(iter(outputs))[0] if outputs else ""

        # output port
        output_port = "None"
        if output_port_name:
            output_port = f"Some(PortID::{output_port_name.upper()})"

        # output
        output = "None"
        if output_port_name:
            output_value = f"\"{step.processors[output_port_name].glob}\""
            output = f"File({output_value}.to_string())"

        # arguments
        arguments = ""
        for arg in step.arguments:
            if isinstance(arg, Port):
                arguments += f"\tPortID::{arg.name.upper()}.into(),\n\t\t"
            else:
                arguments += f"\t\"{arg}\".into(),\n\t\t\t"

        # replace "\" with "\\" in the arguments
        arguments = arguments.replace("\\", "\\\\")

        self.programs[self.current_location.name].write(
            f"""\n
    comm::exec(
      communicator.clone(), // communicator
      "{step.name}".to_string(), // name
      "{step.display_name}".to_string(), // display name
      {output_port}, // output port
      StepOutput::{output}, // output
      "{step.command}".to_string(), // command
      vec![ // arguments
      {arguments}]
    ).await;
    """
        )

    def recv(self, port: str, data_type: str, src: str, dst: str):
      self.programs[self.current_location.name].write(
          f"""
    let {self._get_thread(self.current_location.name)} = comm::receive(communicator.clone(), PortID::{port.upper()}, LocationID::{src.upper()}).await;"""
      )

    def send(self, data: str, port: str, data_type: str, src: str, dst: str):
      self.programs[self.current_location.name].write(
          f"""
    let {self._get_thread(self.current_location.name)} = comm::send(communicator.clone(), PortID::{port.upper()}, LocationID::{dst.upper()}).await;"""
      )

    def seq(self):
        if (
            self.current_location.name in self.thread_stacks.keys()
            and self.thread_stacks[self.current_location.name].get_group()
        ):
            self.programs[self.current_location.name].write(
                f"""\n
    tokio::join!({', '.join(self.thread_stacks[self.current_location.name].delete_group())}); // sequential step
                """
            )
            self.thread_stacks[self.current_location.name].add_group()
        #TODO: sequence the steps
        pass
    
    def begin_paren(self) -> None:
        if self.parallel_step_counter > 1:
            self.parathetized = True

    def end_paren(self):
        self.parathetized = False
        if self.thread_stacks[self.current_location.name].get_group():
            self.programs[self.current_location.name].write(
                f"""\n
    tokio::join!({', '.join(self.thread_stacks[self.current_location.name].delete_group())}); // end parallel step"""
            )
            self.thread_stacks[self.current_location.name].add_group()

    # DONE
    def begin_par(self) -> None:
        if self.parallel_step_counter == 0 and not self.parathetized:
            self.programs[self.current_location.name].write(
                f"""\n
  // begin par
  {self._get_indentation()}let f{self.function_counter} = |communicator: Arc<Communicator>| async move {"{"}"""
            )
            self.functions.append(f"f{self.function_counter}")
            self.function_counter += 1
        self.parallel_step_counter += 1

    def par(self) -> None:
        if (
            self.thread_stacks[self.current_location.name].get_group()
            and not self.parathetized
        ):
            self.programs[self.current_location.name].write(
    f"""
    tokio::join!({', '.join(self.thread_stacks[self.current_location.name].delete_group())}); // parallel step"""
            )
            self.thread_stacks[self.current_location.name].add_group()

            self.thread_stacks[self.current_location.name].add_group()

        if not self.parathetized:
            self.programs[self.current_location.name].write("\n\t};\n")

            self.programs[self.current_location.name].write(
  f"""
  let f{self.function_counter} = |communicator: Arc<Communicator>| async move {"{"}"""
            )
            self.functions.append(f"f{self.function_counter}")
            self.function_counter += 1

    def end_par(self) -> None:
        self.parallel_step_counter -= 1
        if (
            self.thread_stacks[self.current_location.name].get_group()
            and not self.parathetized
        ):
            self.programs[self.current_location.name].write(
    f"""
    tokio::join!({', '.join(self.thread_stacks[self.current_location.name].delete_group())}); // end parallel step"""
            )
            self.thread_stacks[self.current_location.name].add_group()

        if self.parallel_step_counter == 0:
            thread_stack = ThreadStack()

            self.programs[self.current_location.name].write("\n\t};\n")

            while self.functions:
                fun = self.functions.pop()
                thr = thread_stack.add_thread()
                self.programs[self.current_location.name].write(
f"""
  let {thr} = tokio::spawn({fun}(communicator.clone()));"""
                )
            if thread_stack.stack:
                self.programs[self.current_location.name].write(
f"""

  tokio::join!({', '.join(thread_stack.get_group())}); // close macro parallel
"""
                )