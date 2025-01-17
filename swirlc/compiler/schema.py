from __future__ import annotations

from typing import MutableMapping, MutableSequence, TextIO

from swirlc.core.compiler import BaseCompiler
from swirlc.core.entity import Location, Step, Workflow, DistributedWorkflow, Data

class Group:
    def __init__(self) -> None:
        self.thread_stack = []
        self.thread_counter = 0
        self.group_id = 0


class SchemaTarget(BaseCompiler):
    def __init__(self, output_dir: str, env: str) -> None:
        super().__init__()
        self.parathetized = False
        self.programs: MutableMapping[str, TextIO] = {}
        self.workflow: DistributedWorkflow | None = None
        self.output_dir = output_dir
        self.env = env
        
        self.current_location: Location | None = None

        self.group_stack: MutableSequence[Group] = []
        self.group_counter = 0

        self.broadcast_stack: list[(str, str)] = []

    def get_indent(self) -> str:
        return "  " * (len(self.group_stack) - 1)

    def get_thread_name(self) -> str:
        name = "t"
        i = 0
        for group in self.group_stack:
            if i == len(self.group_stack) - 1:
                name += "_" + str(group.thread_counter)
            else:
                name += "_" + str(group.thread_counter - 1)

            i+=1
        return name

        # return f"t{self.group_stack[-1].thread_counter}"
    
    def empty_group_thread_stack(self) -> None:
        group = self.group_stack[-1]

        threads = ", ".join([f"{thread}" for thread in group.thread_stack])
        program = self.programs[self.current_location.name]

        program.write(f"{self.get_indent()}wait {threads}\n")

        group.thread_stack = []

    
    def begin_workflow(self, workflow: Workflow) -> None:
        pass

    def end_workflow(self) -> None:
        pass

    def begin_location(self, location: Location) -> None:
        self.current_location = location

        self.programs[self.current_location.name] = open(
            f"{self.output_dir}/{location.name}.txt", "w"
        )

        # create the main group
        self.group_stack.append(Group())

    def end_location(self) -> None:
        # assert that there is only 1 group left
        assert len(self.group_stack) == 1

        self.empty_group_thread_stack()
    
    def begin_dataset(
        self,
        dataset: MutableSequence[tuple[str, Data]],
    ):
        for port_name, data in dataset:
            self.current_location.data[data.name] = data

    def choice(self):
        raise NotImplementedError("Choice is not implemented yet")

    def exec(
        self,
        step: Step,
        flow: tuple[set[tuple[str, str]], set[tuple[str, str]]],
        mapping: set[str],
    ):
        program = self.programs[self.current_location.name]
        group = self.group_stack[-1]
        
        thread_name = self.get_thread_name()
        program.write(f"{self.get_indent()}let {thread_name} = exec\n")

        group.thread_stack.append(thread_name)
        group.thread_counter += 1
        pass

    def recv(self, port: str, data_type: str, src: str, dst: str):
        program = self.programs[self.current_location.name]
        group = self.group_stack[-1]
        
        thread_name = self.get_thread_name()
        program.write(f"{self.get_indent()}let {thread_name} = recv\n")

        group.thread_stack.append(thread_name)
        group.thread_counter += 1
        pass

    def send(self, data: str, port: str, data_type: str, src: str, dst: str):
        program = self.programs[self.current_location.name]
        group = self.group_stack[-1]
        
        thread_name = self.get_thread_name()
        program.write(f"{self.get_indent()}let {thread_name} = send\n")

        group.thread_stack.append(thread_name)
        group.thread_counter += 1
        pass

    def seq(self):
        # program = self.programs[self.current_location.name]
        # group = self.group_stack[-1]
        
        # # empty the thread stack of the current group and wait for them to finish
        # threads = ", ".join([f"{thread}" for thread in group.thread_stack])
        # program.write(f"{self.get_indent()}wait {threads}\n")
        # group.thread_stack = []
        # pass

        self.empty_group_thread_stack()
    
    def begin_paren(self) -> None:
        program = self.programs[self.current_location.name]
        group = self.group_stack[-1]
        
        self.group_counter += 1
        new_group = Group()
        new_group.group_id = self.group_counter

        thread_name = self.get_thread_name()
        group.thread_stack.append(thread_name)
        group.thread_counter += 1
        
        program.write(f"{self.get_indent()}let {thread_name} = begin group {self.group_counter}\n")
        
        self.group_stack.append(new_group)

    def end_paren(self):
        program = self.programs[self.current_location.name]
        group = self.group_stack[-1]

        # # empty the thread stack and wait for them to finish
        # threads = ", ".join([f"{thread}" for thread in group.thread_stack])
        # program.write(f"{self.get_indent()}wait {threads}\n")
        # group.thread_stack = []

        self.empty_group_thread_stack()

        group = self.group_stack.pop()
        program.write(f"{self.get_indent()}end group {self.group_counter}\n")
        pass
        
    def begin_par(self) -> None:
        pass

    def par(self) -> None:
        pass

    def end_par(self) -> None:
        pass


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

