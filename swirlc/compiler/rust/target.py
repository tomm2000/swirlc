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
from swirlc.compiler.rust.rust_lib import build_locations_module, build_main_file, build_rust_lib
from swirlc.core.compiler import BaseCompiler
from swirlc.core.entity import Location, Step, Port, Workflow, DistributedWorkflow, Data
from swirlc.version import VERSION

# "release" | "debug" | "none"
BUILD_MODE = "none"
ENABLE_BROADCAST = True

class ThreadStack:
    def __init__(self) -> None:
        self.stack: MutableSequence[int] = []

    def top(self) -> int:
        return self.stack[-1]

    def add_thread(self) -> None:
        self.stack[-1] += 1

    def add_group(self) -> None:
        self.stack.append(0)

    def pop_group(self) -> int:
        t = self.stack.pop()
        return t
    
    def clear_group(self) -> None:
        self.stack[-1] = 0
    
    def len(self) -> int:
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

        self.broadcast_stack: dict[str, list[str]] = defaultdict(list)
        self.thread_stack: ThreadStack = ThreadStack()

    def get_indent(self, mod = 0) -> str:
        return "  " * (self.thread_stack.len() + mod)
    
    def begin_workflow(self, workflow: Workflow) -> None:
        self.workflow = workflow

        # remove the build directory if it exists
        # if os.path.exists(f"{self.output_dir}"):
        #     shutil.rmtree(f"{self.output_dir}")

        build_rust_lib(self.output_dir)

        os.makedirs(f"{self.output_dir}/src/locations", exist_ok=True)

    def end_workflow(self) -> None:
        build_run_script(f"{self.output_dir}/run.sh", self.active_locations, self.env, BUILD_MODE, self.output_dir)
        build_config_file(f"{self.output_dir}/src/swirl/config.rs", self.active_locations, self.workflow)
        build_cargo_file(f"{self.output_dir}/Cargo.toml")
        build_main_file(self.output_dir, self.active_locations)
        build_locations_module(self.output_dir, self.active_locations)

        if BUILD_MODE != "none":
            # compile the rust code
            release = "--release" if BUILD_MODE == "release" else ""
            current_dir = os.getcwd()
            os.chdir(self.output_dir)
            os.system(f"RUSTFLAGS=\"-Awarnings\" cargo build {release} --timings")
            os.chdir(current_dir)

    def begin_location(self, location: Location) -> None:
        self.current_location = location
        self.active_locations.append(location)

        start_location_file(f"{self.output_dir}/src/locations/{location.name}.rs", location, self.workflow)

        self.programs[self.current_location.name] = open(
            f"{self.output_dir}/src/locations/{location.name}.rs", "a"
        )

        # create the main group
        self.thread_stack.add_group()

    def end_location(self) -> None:
        # assert that there is only 1 group left (the main group)
        assert self.thread_stack.len() == 1

        program = self.programs[self.current_location.name]

        if ENABLE_BROADCAST: self.empty_broadcast_stack()

        self.wait_thread_group()
        self.thread_stack.pop_group()
        
        program.close()

        close_location_file(f"{self.output_dir}/src/locations/{self.current_location.name}.rs", self.current_location, self.workflow)
    

    def wait_thread_group(self) -> None:
        if self.thread_stack.top() == 0:
            return
        
        self.thread_stack.clear_group()
        self.programs[self.current_location.name].write(
f"""
{self.get_indent()}join_set.join_all().await;
"""
        )

    def refresh_join_set(self) -> None:
        if self.thread_stack.top() == 0:
            self.programs[self.current_location.name].write(
                f"""
{self.get_indent()}let mut join_set = JoinSet::new();
"""
            )
                      

    def begin_dataset(
        self,
        dataset: MutableSequence[tuple[str, Data]],
    ):
        for port_name, data in dataset:
            self.current_location.data[data.name] = data
            if data.type == "file":
                self.programs[self.current_location.name].write(f"""
{self.get_indent()}swirl.init_port("{port_name}".into(), PortData::File("{data.value}".to_string())).await;"""
                )

            elif data.type == "string":
                self.programs[self.current_location.name].write(f"""
{self.get_indent()}communicator.init_port("{port_name}".into(), PortData::String("{data.value}".to_string())).await;
  """
                )

            elif data.type == "int":
                self.programs[self.current_location.name].write(f"""
{self.get_indent()}communicator.init_port("{port_name}".into(), PortData::Int({data.value})).await;
  """
                )

            elif data.type == "bool":
                self.programs[self.current_location.name].write(f"""
{self.get_indent()}communicator.init_port("{port_name}".into(), PortData::Bool({data.value})).await;
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

        outputs = flow[1]
        output_port_name = next(iter(outputs))[0] if outputs else ""

        # output port
        output_port = "None"
        if output_port_name:
            output_port = f"Some(\"{output_port_name}\".into())"

        # output
        output = "StepOutput::None"
        if output_port_name:
            output_value = f"\"{step.processors[output_port_name].glob}\""
            output = f"StepOutput::File({output_value}.to_string())"

        input_ports = ", ".join([f"\"{port_name}\".into()" for port_name, _ in flow[0]])

        # arguments
        arguments = ""
        for arg in step.arguments:
            if isinstance(arg, Port):
                arguments += f"\n{self.get_indent(2)}StepArgument::Port(\"{arg.name}\".into()),"
            else:
                arguments += f"\n{self.get_indent(2)}StepArgument::String(\"{arg}\".into()),"

        # replace "\" with "\\" in the arguments
        arguments = arguments.replace("\\", "\\\\")

        program.write(
f"""
{self.get_indent()}swirl.exec(
{self.get_indent(1)}"{step.name}".to_string(), // name
{self.get_indent(1)}"{step.display_name}".to_string(), // display name
{self.get_indent(1)}vec![{input_ports}], // input ports
{self.get_indent(1)}{output_port}, // output port
{self.get_indent(1)}{output}, // output type
{self.get_indent(1)}"{step.command}".to_string(), // command
{self.get_indent(1)}vec![{arguments}
{self.get_indent(1)}], // arguments
{self.get_indent()}).await;

"""
        )


    def recv(self, port: str, data_type: str, src: str, dst: str):
        program = self.programs[self.current_location.name]
        
        # assigns the receive to a new thread in the current group
        self.refresh_join_set()
        self.thread_stack.add_thread()

        program.write(f"""
{self.get_indent()}join_set = swirl.receive("{port}".into(), "{src}".into(), join_set).await;"""
        )

    def send(self, data: str, port: str, data_type: str, src: str, dst: str):
        program = self.programs[self.current_location.name]

        if ENABLE_BROADCAST:
            self.broadcast_stack[port].append(dst)
        else:
            # assigns the send to a new thread in the current group
            self.refresh_join_set()
            self.thread_stack.add_thread()
            program.write(f"""
{self.get_indent()}join_set = swirl.send("{port}".into(), "{dst}".into(), join_set).await;""")

    def empty_broadcast_stack(self):
        program = self.programs[self.current_location.name]

        # if the stack is empty, do nothing
        if len(self.broadcast_stack) == 0:
            return
        
        for port in self.broadcast_stack:
            self.refresh_join_set()
            self.thread_stack.add_thread()

            destinations = self.broadcast_stack[port]

            # if there is only one destination, use the send method
            if len(destinations) == 1:
                program.write(
                    f"""
{self.get_indent()}join_set = swirl.send("{port}".into(), "{destinations[0]}".into(), join_set).await;""")

            # if there are multiple destinations, use the broadcast method
            else:
                destinations_str = ", ".join([f"\"{destination}\".into()" for destination in destinations])
                destinations_str = f"vec![{destinations_str}]"

                program.write(
                    f"""
{self.get_indent()}join_set = swirl.broadcast("{port}".into(), {destinations_str}, join_set).await;
                    """
                )

        self.broadcast_stack.clear()

    def seq(self):
        program = self.programs[self.current_location.name]

        if ENABLE_BROADCAST: self.empty_broadcast_stack()
        self.wait_thread_group()

        program.write(f"""
{self.get_indent()}//  ===================== sequential step (follows) =====================""")
    
    def begin_paren(self) -> None:
        if ENABLE_BROADCAST: self.empty_broadcast_stack()

        program = self.programs[self.current_location.name]

        self.refresh_join_set()

        program.write(f"""
{self.get_indent()}//  ===================== group start =====================
{self.get_indent()}join_set.spawn({{ let swirl = swirl.clone(); async move {{
""")
        self.thread_stack.add_thread()
        self.thread_stack.add_group()

    def end_paren(self):
        if ENABLE_BROADCAST: self.empty_broadcast_stack()

        program = self.programs[self.current_location.name]

        # wait for the remaining threads in the current group
        self.wait_thread_group()

        # remove the current group from the stack
        self.thread_stack.pop_group()
        
        program.write(
            f"""
{self.get_indent()}}}}});
{self.get_indent()}//  ===================== group end =====================
""")
    
    # the parallel blocks are not explicitly defined in the Rust code, the default behavior is to run in parallel
    def begin_par(self) -> None: pass
    def par(self) -> None: pass
    def end_par(self) -> None: pass


#   join_set.spawn(async move {
#     let join_set =  JoinSet::new();

#     let join_set = swirl.send("l1".into(), "l1".into(), join_set).await;
#     let join_set = swirl.send("l2".into(), "l2".into(), join_set).await;

#     join_set.join_all().await;
#   });

#   join_set.join_all().await;

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

