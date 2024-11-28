from __future__ import annotations

import os
import sys
from typing import MutableMapping, MutableSequence, TextIO
import shutil

from swirlc.compiler.rust.cargo_file import build_cargo_file
from swirlc.compiler.rust.config_file import build_config_file
from swirlc.compiler.rust.run_script import build_run_script
from swirlc.compiler.rust.location_main import start_location_file, close_location_file
from swirlc.core.compiler import BaseCompiler
from swirlc.core.entity import Location, Step, Port, Workflow, DistributedWorkflow, Data
from swirlc.version import VERSION

# "release" or "debug"
BUILD_MODE = "release"

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

    def _get_thread(self, location: str) -> str:
        return self.thread_stacks.setdefault(location, ThreadStack()).add_thread()
    
    def begin_workflow(self, workflow: Workflow) -> None:
        self.workflow = workflow

        shutil.copytree("/rust_base", "./", dirs_exist_ok=True)

        os.makedirs(f"./src/bin", exist_ok=True)

    def end_workflow(self) -> None:
        build_run_script("./run.sh", self.active_locations)
        build_config_file("./src/config.rs", self.active_locations, self.workflow)
        build_cargo_file("./Cargo.toml")

        # compile the rust code
        release = "--release" if BUILD_MODE == "release" else ""
        os.system(f"RUSTFLAGS=\"-Awarnings\" cargo build {release}")

    def begin_location(self, location: Location) -> None:
      self.current_location = location
      self.active_locations.append(location)

      start_location_file(f"./src/bin/{location.name}.rs", location, self.workflow)

      self.programs[self.current_location.name] = open(
          f"./src/bin/{location.name}.rs", "a"
      )

    def end_location(self) -> None:
        if self.thread_stacks[self.current_location.name].get_group():
            self.programs[self.current_location.name].write(
                f"""
    tokio::join!({', '.join(self.thread_stacks[self.current_location.name].delete_group())}); // end location
    """
            )

        self.programs[self.current_location.name].close()

        close_location_file(f"./src/bin/{self.current_location.name}.rs", self.current_location, self.workflow)

    def begin_dataset(
        self,
        dataset: MutableSequence[tuple[str, Data]],
    ):
        for port_name, data in dataset:
            self.current_location.data[data.name] = data
            if data.type == "file":
                self.programs[self.current_location.name].write(f"""
  communicator.init_port(PortID::{port_name.upper()}, PortData::File("{data.value}".to_string())).await;"""
                )

            elif data.type == "string":
                self.programs[self.current_location.name].write(f"""
  communicator.init_port(PortID::{port_name.upper()}, PortData::String("{data.value}".to_string())).await;
  """
                )

            elif data.type == "int":
                self.programs[self.current_location.name].write(f"""
  communicator.init_port(PortID::{port_name.upper()}, PortData::Int({data.value})).await;
  """
                )

            elif data.type == "bool":
                self.programs[self.current_location.name].write(f"""
  communicator.init_port(PortID::{port_name.upper()}, PortData::Bool({data.value})).await;
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

        # input ports
        input_ports = ""
        for port_name, _ in flow[0]:
            input_ports += f"PortID::{port_name.upper()},\n\t\t\t\t"

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
    communicator.exec(
      "{step.name}".to_string(), // name
      "{step.display_name}".to_string(), // display name
      vec![ // input ports
      {input_ports}],
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
    let {self._get_thread(self.current_location.name)} = communicator.receive(PortID::{port.upper()}, LocationID::{src.upper()}).await;"""
      )

    def send(self, data: str, port: str, data_type: str, src: str, dst: str):
      self.programs[self.current_location.name].write(
          f"""
    let {self._get_thread(self.current_location.name)} = communicator.send(PortID::{port.upper()}, LocationID::{dst.upper()}).await;"""
      )

    def seq(self):
        if (
            self.thread_stacks[self.current_location.name].get_group()
        ):
            self.programs[self.current_location.name].write(
                f"""\n
    tokio::join!({', '.join(self.thread_stacks[self.current_location.name].delete_group())}); // sequential step
                """
            )
            self.thread_stacks[self.current_location.name].add_group()
        pass
    
    def begin_paren(self) -> None:
        if self.parallel_step_counter > 1:
            self.parathetized = True

    def end_paren(self):
        self.parathetized = False

        if self.thread_stacks[self.current_location.name].get_group():
            self.programs[self.current_location.name].write(
                f"""\n
    tokio::join!({', '.join(self.thread_stacks[self.current_location.name].delete_group())}); // end parenthetized step"""
            )
            self.thread_stacks[self.current_location.name].add_group()

    def begin_par(self) -> None:
        if self.parallel_step_counter == 0 and not self.parathetized:
            self.programs[self.current_location.name].write(
                f"""\n
  // begin par
  let f{self.function_counter} = |communicator: Arc<Communicator>| async move {"{"}"""
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