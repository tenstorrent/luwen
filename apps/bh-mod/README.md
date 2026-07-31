# bh-mod

Read and modify Blackhole firmware configuration without recompiling
firmware.

Writes go to the `ccfgovr` override banks: two 4 KiB SPI partitions
(`ccfgovra`, `ccfgovrb`) the firmware merges on top of `cmfwcfg` at boot.
The original `cmfwcfg` partition is never touched, so reverting an
override is just a matter of removing it. Any write is followed by a chip
reset so the new config takes effect.

## Build

From the workspace root:

```sh
cargo build --release -p bh-mod
```

The binary lands at `target/release/bh-mod`. Requires the `tenstorrent`
kernel driver loaded, at least one Blackhole device present, and firmware
that supports the `ccfgovr` override mechanism.

## Commands

```
bh-mod [-d <PATH>]... <COMMAND>

  get [-t <TABLE>] [-f pretty|json] [--delta] [FIELDS...]
       print fw-table as `Field | Default | Override`, with Default
       from cmfwcfg and Override from the active ccfgovr bank.
       --delta filters to rows whose Override is set.
       -t read-only / -t flash-info read boardcfg / flshinfo.

  set [-n] FIELD=VALUE...
       merge fields into the override, write to the inactive bank,
       reset the chip.

  res [-n] [-a] [FIELDS...]
       remove fields from the override (cmfwcfg values re-emerge),
       write to the inactive bank, reset the chip.
       -a, --all clears the entire override.
```

Aliases: `set` → `s`, `res` → `r` / `reset`.

- `-d, --dev <PATH>`: target a specific device under `/dev/tenstorrent`.
  Repeatable. Omit to target all detected Blackhole chips.
- `-n, --dry-run`: show what would change without touching flash or
  resetting.

Fields use dot-notation matching the proto layout. The output of
`bh-mod get` is self-documenting: any path it prints is a valid input to
`set` or `res`.

## Validated fields

A few fields accept values the protobuf schema allows but the hardware
cannot honour. `set` rejects those before writing anything, since the
resulting config is applied at boot and published to the host as
harvesting telemetry — and UMD refuses to enumerate a chip reporting more
than one harvested DRAM bank, which takes `tt-smi`, `tt-flash` and
`tt-metal` down with it.

| Field | Accepted | Why |
| --- | --- | --- |
| `dram_table.soft_harvest_dram_mask` | `0`, or one bit in `0..=7` | One bit per GDDR instance, of which there are 8. Firmware soft-harvests at most one. |
| `product_spec_harvesting.dram_disable_count` | `0` or `1` | Firmware clears a single GDDR for this field; a higher count cannot be honoured, and on a part with a fuse-harvested bank it pushes the total to two. |

Rejections happen before any flash write and before the chip reset, so a
refused `set` leaves the override exactly as it was. Values are decimal.

## How `ccfgovr` works

Two 4 KiB banks (`ccfgovra` at `0x1F5000`, `ccfgovrb` at `0x1F6000`) each
store a 20-byte header followed by a `FwTableOverride` protobuf body. The
firmware decodes the bank with the newer plausible sequence number into a
`FwTableOverride` struct and applies each present field on top of the
`cmfwcfg`-loaded `FwTable`. The other bank is the fallback if a write to
the active bank tears.

`bh-mod set` and `bh-mod res` always write to the **inactive** bank,
leaving the active one intact as the rollback target. The new bank then
becomes active on the next boot.

The set of modifiable fields is defined by `fw_table_override.proto` —
only fields explicitly exposed there can be set via `bh-mod`. Adding a
new field requires editing the proto, the firmware's per-field merge
list, and re-flashing.

## Examples

```sh
# fw-table view: all fields with their cmfwcfg defaults and active overrides
bh-mod get

# Only fields with an active override set
bh-mod get --delta

# One field, default + override side-by-side
bh-mod get chip_limits.tdp_limit

# As JSON
bh-mod get -f json chip_limits.tdp_limit

# Board config for a specific device
bh-mod -d /dev/tenstorrent/0 get -t read-only

# Set a field (dry-run first)
bh-mod set -n chip_limits.asic_fmax=1350
bh-mod set chip_limits.asic_fmax=1350

# Stack a second override; both apply on next boot (when more fields
# are exposed in fw_table_override.proto)
bh-mod set chip_limits.tdp_limit=160

# Remove one override (cmfwcfg value re-emerges)
bh-mod res chip_limits.tdp_limit

# Clear the entire override
bh-mod res --all
```

## Multi-chip output

When multiple devices are targeted, `get` and the diff tables in
`set`/`res` show one column per chip and collapse columns whose values
agree:

```
+----------------------+----------+----------+
| Field                | chip 0   | chip 1   |
+----------------------+----------+----------+
| chip_limits.asic_fmax| 1350     | 1200     |
+----------------------+----------+----------+
```

If every chip agrees, the columns collapse to a single `Value` column.

## Known limitations

- **Only fields in `fw_table_override.proto` are modifiable.** Setting
  any other field (including `fw_bundle_version`, anything in
  `feature_enable`, etc.) is rejected at the CLI level. The override
  surface area is intentionally narrow and grows only by deliberate
  edits to the proto.
- **Override body size is capped at 512 bytes** (firmware decode limit).
  A handful of fields fits comfortably; setting nearly every exposed
  field will eventually hit the cap.
