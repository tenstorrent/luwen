# bh-mod

Read and modify Blackhole SPI flash protobuf tables without recompiling
firmware.

Any write (`set`, `res`) is followed by a chip reset so the new config takes
effect.

## Tables

| Name         | Tag         | Access     |
| ------------ | ----------- | ---------- |
| `fw-table`   | `cmfwcfg`   | read/write |
| `read-only`  | `boardcfg`  | read-only  |
| `flash-info` | `flshinfo`  | read-only  |

Factory defaults live in `origcfg` and are the source for `res`.

## Build

From the workspace root:

```sh
cargo build --release -p bh-mod
```

The binary lands at `target/release/bh-mod`. Requires the `tenstorrent`
kernel driver loaded and at least one Blackhole device present.

## Commands

```
bh-mod [-d <PATH>]... <COMMAND>

  get [-t <TABLE>] [-f pretty|json] [FIELDS...]   print a table
  set [-n] FIELD=VALUE...                         write fields to fw-table
  res [-n] [-a] [FIELDS...]                       restore fields from origcfg
```

Aliases: `set` → `s`, `res` → `r` / `reset`.

- `-d, --dev <PATH>`: target a specific device under `/dev/tenstorrent`.
  Repeatable. Omit to target all detected Blackhole chips.
- `-n, --dry-run`: show what would change without touching flash or resetting.
- `-a, --all` (res): restore every field.

Fields use dot-notation matching the proto layout. The output of
`bh-mod get` is self-documenting: any path it prints is a valid input to
`set` or `res`.

## Examples

```sh
# Print fw-table for every Blackhole in the system
bh-mod get

# Print one field as JSON
bh-mod get -f json chip_limits.asic_fmax

# Print the read-only board config for a specific device
bh-mod -d /dev/tenstorrent/0 get -t read-only

# Set a field (dry-run first)
bh-mod set -n chip_limits.asic_fmax=1350
bh-mod set chip_limits.asic_fmax=1350

# Restore one field, or everything, from factory defaults
bh-mod res chip_limits.asic_fmax
bh-mod res --all
```

## Multi-chip output

When multiple devices are targeted, `get` and the diff tables in `set`/`res`
show one column per chip and collapse columns whose values agree:

```
+----------------------+----------+----------+
| Field                | chip 0   | chip 1   |
+----------------------+----------+----------+
| chip_limits.asic_fmax| 1350     | 1200     |
+----------------------+----------+----------+
```

If every chip agrees, the columns collapse to a single `Value` column.
