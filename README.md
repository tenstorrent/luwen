# Luwen

Named after Antonie van Leeuwenhoek who invented the microsope.

## Official Repository

[https://github.com/tenstorrent/luwen](https://github.com/tenstorrent/luwen)

## Prometheus Exporter

Luwen includes a Prometheus exporter for hardware telemetry (see [`app/prometheus-exporter/`](app/prometheus-exporter/)).

To start it:

```bash
cargo build -p prometheus-exporter --release
./target/release/prometheus-exporter
```

Metrics are exposed at `http://localhost:8080/metrics`.

## Design

There are three usecases that I want to support here

1. High level interface to software tooling allowing all syseng diagnostics collectable via tt-smi and tt-mod to be
   readback and interacted with as a library. - This will only be a high level interface so it will only support pci connections and remote connections via pci - Will ignore all details of using communication channels such as which pci tlb or which erisc core to use.
1. General chip discovery and init, replacing create-ethernet-map and tt-smi wait. We'll probably also add the ability
   to issue resets.
1. Low level syseng-only debug capability (via pyluwen)
   - To avoid needing to have multiple in flight stacks you will be able to drop down a level and access the
     implemented communication apis directly. Practically this means direct access to the types defined in luwen-ref.
     This means that you can modify pci tlbs and erisc cores being used or cut out the middle man entirely and
     issue raw calls.
