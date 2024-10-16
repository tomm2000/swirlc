#!/usr/bin/env python
# -*- coding: utf-8 -*-

# This file was generated automatically using SWIRL v0.0.1,
# using command swirlc compile .\examples\example2\example2.swirl .\examples\example2\config.yml

from __future__ import annotations

import glob
import logging
import os
import socket
import subprocess
import time
import uuid

from io import BytesIO
from pathlib import Path
from threading import Condition, Event, Thread
from typing import Any, MutableMapping, MutableSequence


BUF_SIZE = 8192

available_port_data = {}
condition: Condition = Condition()
connections: MutableMapping[str, MutableMapping[str, socket]] = {}
ports: MutableMapping[str, Any] = {}
stopping: bool = False


logger = logging.getLogger("swirlc")
defaultStreamHandler = logging.StreamHandler()
formatter = logging.Formatter(
    fmt="%(asctime)s.%(msecs)03d %(filename)s %(levelname)-8s %(message)s",
    datefmt="%Y-%m-%d %H:%M:%S",
)
defaultStreamHandler.setFormatter(formatter)
logger.addHandler(defaultStreamHandler)
logger.setLevel(logging.DEBUG)
logger.propagate = False


def _accept(sock: socket):
    while not stopping:
        try:
            conn, _ = sock.accept()
            name, port = conn.recv(1024).decode("utf-8").split()
            if logger.isEnabledFor(logging.DEBUG):
                logger.debug(
                    f"Accepted connection for port {port} from location {name}"
                )
            with condition:
                connections.setdefault(name, {})[port] = conn
                conn.send("ack".encode("utf-8"))
                condition.notify_all()
        except socket.timeout:
            pass
    sock.close()


def _exec(
    step_name: str,
    step_display_name: str,
    input_port_names: MutableSequence[str],
    output_port_name: str,
    data_type: str,
    glob_regex: str | None,
    cmd: str,
    args: MutableSequence[str],
    args_from: MutableSequence[tuple[str, str]],
    workdir: str,
):
    # Wait all the data
    for port_name in input_port_names:
        available_port_data[port_name].wait()
    # Prepare working directory
    workdir = os.path.join(workdir, f"exec_{step_name}_{uuid.uuid4()}")
    os.mkdir(workdir)
    for port_name in input_port_names:
        os.symlink(
            os.path.abspath(ports[port_name]),
            os.path.join(workdir, os.path.basename(ports[port_name])),
        )
    # Populate the arguments
    arguments = []
    if (len_args := len(args)) > 0:
        args = iter(args)
    if (len_args_from := len(args_from)) > 0:
        args_from = iter(args_from)
        elem = next(args_from)
        next_pos, next_port = elem
    else:
        next_pos, next_port = -1, None
    for i in range(len_args + len_args_from):
        if i == next_pos:
            arguments.append(ports[next_port])
            if i < len_args_from - 1:
                next_pos, next_port = next(args_from)
            else:
                next_pos, next_port = -1, None
        else:
            arguments.append(next(args))
    cmd = " ".join((cmd, *arguments))
    if logger.isEnabledFor(logging.INFO):
        logger.info(f"Step {step_display_name}-{step_name} executes command '{cmd}'")
    result = subprocess.run(cmd, capture_output=True, shell=True, cwd=workdir)
    if result.returncode != 0:
        raise Exception(
            f"Step {step_display_name}-{step_name} failed with exit status {result.returncode}: {result.stderr.decode('utf-8')}"
        )
    if output_port_name:
        if data_type == "stdout":
            ports[output_port_name] = result.stdout
            if logger.isEnabledFor(logging.INFO):
                logger.info(
                    f"Step {step_display_name}-{step_name} result: '{result.stdout.decode().strip()}'"
                )
        elif data_type in ("file", "directory"):
            res = [path for path in glob.glob(os.path.join(workdir, glob_regex))]
            if len(res) == 0:
                raise FileNotFoundError(
                    f"Step {step_display_name}-{step_name} did not produce a file or directory which match the glob regex: {glob_regex}"
                )
            elif len(res) == 1:
                ports[output_port_name] = os.path.join(workdir, res[0])
                if logger.isEnabledFor(logging.INFO):
                    logger.info(
                        f"Step {step_display_name}-{step_name} result file: '{ports[output_port_name]}'"
                    )
            else:
                raise Exception(
                    f"Step {step_display_name}-{step_name} produced too many files or directories which match glob regex: {res}"
                )
        else:
            raise Exception(f"Unsupported data type: {data_type}")
        available_port_data[output_port_name].set()
    else:
        if logger.isEnabledFor(logging.INFO):
            logger.info(
                f"Step {step_display_name}-{step_name} has not an output port. Result: '{result.stdout.decode().strip()}'"
            )


def _init_dataset(port_name: str, data: str):
    ports[port_name] = data
    available_port_data[port_name] = Event()
    available_port_data[port_name].set()


def _send(port: str, data_type: str, src: str, dst: str):
    while True:
        try:
            sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            sock.connect(locations[dst])
            break
        except socket.error:
            time.sleep(1)
    sock.send(f"{src} {port}".encode("utf-8"))
    sock.recv(BUF_SIZE)
    if data_type == "stout":
        sock.send(ports[port])
    elif data_type == "file":
        sock.send(os.path.basename(ports[port]).encode("utf-8"))
        sock.recv(BUF_SIZE)
        fd = open(ports[port], "rb")
        while True:
            buf = fd.read(BUF_SIZE)
            if not buf:
                break
            sock.sendall(buf)
        fd.close()
    elif data_type == "directory":
        raise NotImplementedError(f"Recv directories not implemented yet")
    else:
        raise Exception(f"Unsupported data type: {data_type}")
    if logger.isEnabledFor(logging.DEBUG):
        logger.debug(f"Sent data for port {port} to location {dst}")
    sock.close()


def _recv(port: str, workdir: str, data_type: str, src: str) -> Any:
    buf = BytesIO()
    with condition:
        while connections.setdefault(src, {}).get(port) is None:
            logger.debug(f"Waiting connection for port {port} from location {src}")
            condition.wait()
    if logger.isEnabledFor(logging.DEBUG):
        logger.debug(f"Received connection for port {port} from location {src}")
    if data_type == "stdout":
        while True:
            if not (data := connections[src][port].recv(BUF_SIZE)):
                break
            buf.write(data)
        if logger.isEnabledFor(logging.DEBUG):
            logger.debug(f"Received data for port {port} from location {src}")
        buf.seek(0)
        ports[port] = buf.read().decode("utf-8")
        available_port_data.setdefault(port, Event()).set()
    elif data_type == "file":
        filename = connections[src][port].recv(1024).decode()
        connections[src][port].send("ack".encode("utf-8"))
        filepath = os.path.join(workdir, f"rcv_{port}_{uuid.uuid4()}", filename)
        os.mkdir(os.path.dirname(filepath))
        fd = open(filepath, "wb")
        while True:
            if not (data := connections[src][port].recv(BUF_SIZE)):
                break
            fd.write(data)
        fd.close()
        ports[port] = filepath
        available_port_data.setdefault(port, Event()).set()
        logger.debug(f"Received file '{ports[port]}' on port {port}")
    elif data_type == "directory":
        raise NotImplementedError(f"Recv directories not implemented yet")
    else:
        raise Exception(f"Unsupported data type: {data_type}")
    connections[src][port].close()
    connections[src][port] = None


def _thread(f, *args) -> Thread:
    thread = Thread(target=f, args=args)
    thread.start()
    return thread


def _wait(threads: MutableSequence[Thread]):
    for t in threads:
        t.join()


def main():
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.bind(locations["ld"])
    sock.settimeout(3)
    sock.listen(3)

    _thread(_accept, sock)

    _init_dataset("p1", "world.txt")
    available_port_data.setdefault("p2", Event())
    input_port_names = ["p1"]
    for port_name in input_port_names:
        available_port_data.setdefault(port_name, Event())
    _exec(
        "s1",
        "FirstStep",
        input_port_names,
        "p2",
        "file",
        "hello.txt",
        "cat",
        ["> hello.txt"],
        [(0, "p1")],
        str(Path("None").expanduser().absolute()),
    )

    def f0():
        t0 = _thread(_send, "p2", "file", "ld", "l1")
        _wait([t0])

    def f1():
        t1 = _thread(_send, "p2", "file", "ld", "l2")
        _wait([t1])

    t0 = _thread(f1)
    t1 = _thread(f0)
    _wait([t1, t0])
    logger.info("Terminated trace")
    global stopping
    stopping = True


locations = {
    "ld": ("127.0.0.1", 8080),
    "l1": ("127.0.0.1", 8081),
    "l2": ("127.0.0.1", 8082),
    "l3": ("127.0.0.1", 8083),
}

if __name__ == "__main__":
    main()
