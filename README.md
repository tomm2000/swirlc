# SWIRL-rs: A Rust execution target for SWIRL, a Scientific Workflow Intermediate Representation Language.
The original [repository](https://github.com/alpha-unito/swirlc)  contains a Python implementation for the swirlc toolchain. Swirlc includes a translator to generate SWIRL workflows from Pegasus DAX files and a compiler to generate a Python executable bundle from a SWIRL workflow. This fork extends the swirlc compiler to generate a Rust executable bundle instead of a Python one. The bulk of the changes are in the `swirlc/compiler/rust` folder, this includes the Rust library code (under `swirlc/compiler/rust/src`) and the python compiler scripts.

## Installation
The repository contains a Dockerfile that can be used to build an image with the swirlc toolchain. To start clone the repository:

```bash
git clone https://github.com/tomm2000/swirlc
```

Then build the Docker image:

```bash
docker build -t swirlc-rust .
```

## Generate the Rust executable
The following command generates the Rust workflow source code starting from a SWIRL workflow and a configuration file (instructions on how to obtain these files can be found in the original [repository](https://github.com/tomm2000/swirlc?tab=readme-ov-file#translate)).
```bash
docker run                                        \
        --platform linux/x86_64                   \
        -u $(id -u):$(id -g)                      \
        -v $(pwd)/<WORKFLOW_LOCATION>:/src        \
        -v $(pwd)/build:/build                    \
        -w /                                      \
        swirlc-rust                               \
    swirlc                                        \
        compile                                   \
        --target rust                             \
        /src/<WORKFLOW>.swirl                     \
        /src/<CONFIG>.yml
```

The generated Rust source code will be in the `build` folder, it can be compiled with the Rust cargo toolchain.

```bash
cargo build --release
```

The `build` folder will contain an example `.sh` script to execute the generated Rust executable on a SLURM cluster.

## Examples
The `examples` folder contains some synthetic workflows that can be used to test the toolchain. Along with pre-generated workflows, a python script is provided to generate new workflows with different sizes.