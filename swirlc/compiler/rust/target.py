from __future__ import annotations

from collections import defaultdict
import os
import sys
from typing import MutableMapping, MutableSequence, TextIO
import shutil
from ruamel.yaml import YAML

from swirlc.compiler.rust.cargo_file import build_cargo_file
from swirlc.compiler.rust.config_file import build_config_file
from swirlc.compiler.rust.run_script import build_run_script
from swirlc.compiler.rust.location_main import start_location_file, close_location_file
from swirlc.compiler.rust.rust_lib import build_rust_lib
from swirlc.core.compiler import BaseCompiler
from swirlc.core.entity import Location, Step, Port, Workflow, DistributedWorkflow, Data
from swirlc.version import VERSION

BUILD_MODE = "release"

class Group:
    def __init__(self) -> None:
        self.thread_stack = []
        self.thread_counter = 0
        self.group_id = 0
        self.function_name = ""

    def add_thread(self, thread: str) -> None:
        self.thread_stack.append(thread)
        self.thread_counter += 1

    def empty_thread_stack(self) -> None:
        self.thread_stack = []

class GroupStack:
    def __init__(self) -> None:
        self.stack: list[Group] = []
        self.counter = 0

    def push(self) -> Group:
        group = Group()
        group.group_id = self.counter - 1
        self.stack.append(group)
        self.counter += 1
        return group
    
    def pop(self) -> Group:
        return self.stack.pop()
    
    def top(self) -> Group:
        return self.stack[-1]

    def depth(self) -> int:
        return len(self.stack)


class RustTarget(BaseCompiler):
    def __init__(self, output_dir: str, env: str) -> None:
        super().__init__()
        self.parathetized = False
        self.programs: MutableMapping[str, TextIO] = {}
        self.workflow: DistributedWorkflow | None = None
        self.output_dir = output_dir
        self.env = env
        
        self.current_location: Location | None = None
        self.active_locations: MutableSequence[Location] = []

        self.group_stack: GroupStack = GroupStack()

    def get_indent(self, mod = 0) -> str:
        return "  " * (self.group_stack.depth() + mod)

    def get_thread_name(self, type: str = "task") -> str:
        name = type
        i = 0
        for group in self.group_stack.stack:
            # if its the last group
            if i == len(self.group_stack.stack) - 1:
                name += "_" + str(group.thread_counter)
            else:
                name += "_" + str(group.thread_counter - 1)

            i+=1
        return name

    def empty_group_thread_stack(self) -> None:
        group = self.group_stack.top()

        threads = ", ".join([f"{thread}" for thread in group.thread_stack])
        program = self.programs[self.current_location.name]
        
        group.empty_thread_stack()

        if threads:
            program.write(
                f"""\n\n{self.get_indent()}tokio::join!({threads});""")

    
    def begin_workflow(self, workflow: Workflow) -> None:
        self.workflow = workflow

        build_rust_lib(self.output_dir)

        os.makedirs(f"{self.output_dir}/src/bin", exist_ok=True)

    def end_workflow(self) -> None:
        build_run_script(f"{self.output_dir}/run.sh", self.active_locations, self.env, BUILD_MODE, self.output_dir)
        build_config_file(f"{self.output_dir}/src/config.rs", self.active_locations, self.workflow)
        build_cargo_file(f"{self.output_dir}/Cargo.toml")

        # compile the rust code
        release = "--release" if BUILD_MODE == "release" else ""
        current_dir = os.getcwd()
        os.chdir(self.output_dir)
        os.system(f"RUSTFLAGS=\"-Awarnings\" cargo build {release}")
        os.chdir(current_dir)

    def begin_location(self, location: Location) -> None:
        self.group_stack = GroupStack()

        self.current_location = location
        self.active_locations.append(location)

        start_location_file(f"{self.output_dir}/src/bin/{location.name}.rs", location, self.workflow)

        self.programs[self.current_location.name] = open(
            f"{self.output_dir}/src/bin/{location.name}.rs", "a"
        )

        # create the main group
        self.group_stack.push()

    def end_location(self) -> None:
        # assert that there is only 1 group left (the main group)
        assert self.group_stack.depth() == 1

        self.empty_group_thread_stack()

        self.group_stack.pop()
        program = self.programs[self.current_location.name]
        program.close()

        close_location_file(f"{self.output_dir}/src/bin/{self.current_location.name}.rs", self.current_location, self.workflow)
        
    
    def begin_dataset(
        self,
        dataset: MutableSequence[tuple[str, Data]],
    ):
        for port_name, data in dataset:
            self.current_location.data[data.name] = data
            if data.type == "file":
                self.programs[self.current_location.name].write(f"""
{self.get_indent()}communicator.init_port(PortID::{port_name.upper()}, PortData::File("{data.value}".to_string())).await;"""
                )

            elif data.type == "string":
                self.programs[self.current_location.name].write(f"""
{self.get_indent()}communicator.init_port(PortID::{port_name.upper()}, PortData::String("{data.value}".to_string())).await;
  """
                )

            elif data.type == "int":
                self.programs[self.current_location.name].write(f"""
{self.get_indent()}communicator.init_port(PortID::{port_name.upper()}, PortData::Int({data.value})).await;
  """
                )

            elif data.type == "bool":
                self.programs[self.current_location.name].write(f"""
{self.get_indent()}communicator.init_port(PortID::{port_name.upper()}, PortData::Bool({data.value})).await;
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
        program = self.programs[self.current_location.name]
        group = self.group_stack.top()
        
        # assigns the execution to a new thread in the current group
        # thread_name = self.get_thread_name()
        # group.thread_stack.append(thread_name)
        # group.thread_counter += 1

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
            input_ports += f"\n{self.get_indent()}\t\tPortID::{port_name.upper()},"

        # arguments
        arguments = ""
        for arg in step.arguments:
            if isinstance(arg, Port):
                arguments += f"\n{self.get_indent()}\t\tPortID::{arg.name.upper()}.into(),"
            else:
                arguments += f"\n{self.get_indent()}\t\t\"{arg}\".into(),"

        # replace "\" with "\\" in the arguments
        arguments = arguments.replace("\\", "\\\\")

        program.write(
            f"""\n
{self.get_indent()}communicator.exec(
{self.get_indent()}  "{step.name}".to_string(), // name
{self.get_indent()}  "{step.display_name}".to_string(), // display name
{self.get_indent()}  vec![ // input ports {input_ports}
{self.get_indent()}  ],
{self.get_indent()}  {output_port}, // output port
{self.get_indent()}  StepOutput::{output}, // output
{self.get_indent()}  "{step.command}".to_string(), // command
{self.get_indent()}  vec![ // arguments {arguments}
{self.get_indent()}  ]
{self.get_indent()}).await;"""
        )

    def recv(self, port: str, data_type: str, src: str, dst: str):
        program = self.programs[self.current_location.name]
        group = self.group_stack.top()
        
        # assigns the receive to a new thread in the current group
        thread_name = self.get_thread_name("recv")
        group.add_thread(thread_name)

        program.write(
            f"""
{self.get_indent()}let {thread_name} = communicator.receive(PortID::{port.upper()}, LocationID::{src.upper()}).await;"""
        )

    def send(self, data: str, port: str, data_type: str, src: str, dst: str):
        program = self.programs[self.current_location.name]
        group = self.group_stack.top()
        
        # assigns the send to a new thread in the current group
        thread_name = self.get_thread_name("send")
        group.add_thread(thread_name)

        program.write(
            f"""
{self.get_indent()}let {thread_name} = communicator.send(PortID::{port.upper()}, LocationID::{dst.upper()}).await;"""
        )

    def seq(self):
        program = self.programs[self.current_location.name]
        group = self.group_stack.top()

        
        self.empty_group_thread_stack()
        program.write(f"""
//  ===================== sequential step (follows) =====================""")
    
    def begin_paren(self) -> None:
        program = self.programs[self.current_location.name]
        group = self.group_stack.top()
        thread_name = self.get_thread_name("group")
        group.add_thread(thread_name)
        
        # creates a new group...
        new_group = self.group_stack.push()

        program.write(
            f"""\n
// ===================== group #{new_group.group_id} start =====================
{self.get_indent(-1)}let {thread_name} = tokio::spawn({{ let communicator = communicator.clone(); async move {{"""
        )

    def end_paren(self):
        program = self.programs[self.current_location.name]

        # wait for the remaining threads in the current group
        self.empty_group_thread_stack()

        # remove the current group from the stack
        group = self.group_stack.pop()
        
        # spawn group as a new thread in the current group
        
        program.write(
            f"""
{self.get_indent()}}}}});
//  ===================== group #{group.group_id} end =====================
""")
    
    # the parallel blocks are not explicitly defined in the Rust code, the default behavior is to run in parallel
    def begin_par(self) -> None: pass
    def par(self) -> None: pass
    def end_par(self) -> None: pass


# (
#   send(d0 ->p26,l0,l5) | send(d18->p30,l0,l1) | send(d1->p27,l0,l1) | send(d1 ->p27,l0,l8) | send(d0->p26,l0,l1) |
#   send(d1 ->p27,l0,l4) | send(d1 ->p27,l0,l3) | send(d1->p27,l0,l7) | send(d0 ->p26,l0,l2) | send(d0->p26,l0,l7) |
#   send(d21->p31,l0,l2) | send(d0 ->p26,l0,l8) | send(d0->p26,l0,l3) | send(d27->p33,l0,l4) | send(d0->p26,l0,l6) |
#   send(d25->p32,l0,l3) | send(d0 ->p26,l0,l9) | send(d1->p27,l0,l6) | send(d34->p35,l0,l6) | send(d1->p27,l0,l2) |
#   send(d30->p34,l0,l5) | send(d1 ->p27,l0,l5) | send(d1->p27,l0,l9) | send(d0 ->p26,l0,l4)
# )
# |
# (
#   exec(s0,{(p26,d0),(p27,d1)}->{(p0,d2)},{l0})
# )
# |
# (
#   (
#       recv(p1,l1,l0) | recv(p6,l6,l0) | recv(p3,l3,l0) | recv(p9,l9,l0) | recv(p8,l8,l0) | 
#       recv(p2,l2,l0) | recv(p5,l5,l0) | recv(p7,l7,l0) | recv(p4,l4,l0)
#   )
#   .
#   exec(s10,{(p0,d2),(p1,d3),(p2,d4),(p3,d5),(p4,d6),(p5,d7),(p6,d8),(p7,d9),(p8,d10),(p9,d11)}->{(p10,d12)},{l0})
#   .
#   (
#       send(d12->p10,l0,l4) | send(d12->p10,l0,l1) | send(d12->p10,l0,l5) |
#       send(d12->p10,l0,l3) | send(d12->p10,l0,l6) | send(d12->p10,l0,l2)
#   )
# )
# |
# (
#   exec(s11,{(p28,d13)}->{(p11,d14)},{l0})
#   .
#   (
#       send(d14->p11,l0,l4) | send(d14->p11,l0,l3) | send(d14->p11,l0,l2) |
#       send(d14->p11,l0,l6) | send(d14->p11,l0,l1) | send(d14->p11,l0,l5)
#   )
# )
# |
# (
#   exec(s12,{(p10,d12),(p11,d14),(p27,d1),(p29,d16)}->{(p12,d15)},{l0})
# )
# |
# (
#   exec(s13,{(p12,d15)}->{},{l0})
# )
# |
# (
#   exec(s14,{(p10,d12),(p11,d14),(p27,d1),(p29,d16)}->{(p13,d17)},{l0})
# )
# |
# (
#   exec(s15,{(p13,d17)}->{},{l0})
# )

