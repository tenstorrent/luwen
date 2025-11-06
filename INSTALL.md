# Install

This document provides development instructions for building and installing
various distributions.

## Dependencies

The following are required dependencies and must be installed on your system to
build Luwen.

### [`protoc`](https://protobuf.dev)

The protocol buffer compiler can be installed either by downloading a
pre-compiled binary to your `$PATH` or by using your package manager. See the
[installation instructions][protoc.install] for more details.

[protoc.install]: https://protobuf.dev/installation

## Libraries

### Rust

Luwen is written as a Rust crate (library) called `luwen`. To get started,
simply add the crate to your project using the library distribution from
[crates.io](https://crates.io/crate/luwen):

```shell
cargo add luwen
```

#### Build

There should be no need for end-users to build the library crate directly,
however, this can be accomplished by cloning this repository, and building with
Cargo:

```shell
cargo build --release -pluwen
```

The compiled library should be available under `./target/release` as
`libluwen.rlib`.

### Python

Bindings to Luwen are available through the `pyluwen` Python package. This can
be added to your Python environment using `pip`:

```shell
pip install pyluwen
```

#### Build

> [!IMPORTANT]
>
> It is always recommended to manage, build, and install Python packages within
> a virtual environment. Doing otherwise may override system packages and
> potentially interfere with your computer's OS.
>
> A virtual environment can be created using `venv`:
>
> ```shell
> # Create a virtual environment
> python3 -m venv .venv
>
> # Activate the environment
> . .venv/bin/activate
> ```

To build the `pyluwen` package yourself, run `pip` from within your Python
virtual environment to automatically install dependencies and build the package:

```shell
pip install -v bind/pyluwen
```
