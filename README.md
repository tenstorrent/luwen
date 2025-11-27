<h1 align="center">
  <p>Luwen</p>
</h1>

<p align="center">
  Named after Antonie van Leeuwenhoek, inventor of the microscope.
</p>

[![documentation][docs.badge]][docs.hyper]
[![crates.io pkg][rust.badge]][rust.hyper]
[![pypi bindings][pypi.badge]][pypi.hyper]

[docs.badge]: https://img.shields.io/docsrs/luwen
[docs.hyper]: https://docs.rs/crate/luwen
[pypi.badge]: https://img.shields.io/pypi/v/pyluwen
[pypi.hyper]: https://pypi.org/project/pyluwen
[rust.badge]: https://img.shields.io/crates/v/luwen
[rust.hyper]: https://crates.io/crates/luwen

## About

Luwen is a user-mode abstraction layer for accessing Tenstorrent accelerator
hardware, designed to be used as a common interface for tooling development
across hardware generations. It runs on the host system and manages
communication with lower-level drivers and firmware.

## Design

There are three supported use cases:

1. High-level interface to software tooling allowing all syseng diagnostics
   collectable via `tt-smi` and `tt-mod` to be read back and interacted with as a
   library.
   - Only supports PCIe connections and remote connections via PCI.
   - Ignores implementation details of using communication channels, such as
     TLB allocation and core selection.
1. General chip discovery and initialization, replacing `create-ethernet-map`
   and `tt-smi`. Will probably also add the ability to issue resets.
1. Low-level syseng-only debug capability (via Python bindings, as `pyluwen`).
   - To avoid needing to have multiple in-flight stacks, access the implemented
     communication APIs directly.
   - Direct access to the types defined in `luwen-pci`, allowing modification
     of TLBs and cores being used.

## Installation

See the included [`INSTALL.md`](./INSTALL.md) for detailed instructions on how
to build and install Luwen.

## Support

### Hardware

Luwen officially supports the following Tenstorrent hardware:

- Wormhole
- Blackhole

#### [Firmware]

Please ensure you have a supported firmware version installed on your device.
You can flash the firmware using [`tt-flash`][tt-flash].

> [!IMPORTANT]
>
> The current minimum supported firmware version is: `v18`.

[firmware]: https://github.com/tenstorrent/tt-firmware
[tt-flash]: https://github.com/tenstorrent/tt-flash

#### [Driver (KMD)][driver]

Luwen communicates with your device through a kernel-mode driver. You can find
instructions on installing the driver on the project [homepage][driver].

> [!IMPORTANT]
>
> The current minimum supported driver version is: `v2.0.0`.

[driver]: https://github.com/tenstorrent/tt-kmd

## Organization

Cargo — Rust's package manager — allows for a workspace of several crates to be
specified within its [manifest](./Cargo.toml). Within this project, workspace
crates are used with the structure as follows:

```
./
├── Cargo.lock       # cargo lockfile
├── Cargo.toml       # cargo manifest
├── README.md        # this document
├── ...
├── apps/            # use-case applications
├── bind/            # language bindings
│   ├── libluwen/    # bindings for C++
│   └── pyluwen/     # bindings for Python
├── crates/          # implementation crates
│   ├── luwen-api/   # core generalized API
│   ├── luwen-def/   # common definitions
│   ├── luwen-kmd/   # low-level driver API
│   └── luwen-pci/   # PCI implementation
├── examples/        # application examples
├── src/             # top-level library
└── tests/           # integration tests
```

## License

This project is licensed under [Apache License 2.0](./LICENSE). You have
permission to use this code under the conditions of the license pursuant to the
rights it grants.

This software assists in programming Tenstorrent products. Making, using, or
selling hardware, models, or IP may require the license of rights (such as
patent rights) from Tenstorrent or others.
