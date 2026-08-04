//! Value constraints on `ccfgovr` fields that the protobuf schema cannot
//! express.
//!
//! `fw_table_override.proto` types every harvesting field as a plain
//! `uint32`, so the schema accepts values the hardware cannot honour and
//! nothing between the keystroke and the next boot objects. Firmware
//! applies whatever it finds to `tile_enable.gddr_enabled`, telemetry
//! publishes that as `TAG_ENABLED_GDDR`, and UMD then refuses to build a
//! coordinate manager for a chip reporting more than one harvested DRAM
//! bank — aborting topology discovery for *every* device, so `tt-smi`,
//! `tt-flash` and `tt-metal` all report no chips at all.
//!
//! These checks run before anything is staged for writing, so a rejected
//! value leaves flash untouched and the chip un-reset.

use anyhow::Context as _;
use serde_json::Value;

/// Number of GDDR instances on a Blackhole chip. `soft_harvest_dram_mask`
/// carries one bit per instance, so bits 8..=31 name nothing — firmware
/// drops them silently when it ANDs the mask into the `uint8_t`
/// `tile_enable.gddr_enabled`.
const NUM_GDDR: u32 = 8;

/// Most DRAM banks that may end up harvested on Blackhole. Firmware
/// soft-harvests at most one GDDR instance, and UMD refuses to enumerate a
/// chip reporting more than one harvested bank. The messages below are
/// worded for a value of 1; revisit them if this ever changes.
const MAX_HARVESTED_DRAM: u32 = 1;

/// Interpret a field's value as the `uint32` its protobuf type promises.
fn as_u32(path: &str, value: &Value) -> anyhow::Result<u32> {
    value
        .as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .with_context(|| format!("{path} must be a 32-bit non-negative integer"))
}

/// Check one `field=value` assignment against the constraints for that
/// field. Paths without constraints are accepted unchanged.
pub fn field(path: &str, value: &Value) -> anyhow::Result<()> {
    match path {
        "dram_table.soft_harvest_dram_mask" => {
            let mask = as_u32(path, value)?;
            anyhow::ensure!(
                (mask >> NUM_GDDR) == 0,
                "{path}: 0x{mask:x} sets bits above bit {last}, but this chip has only \
                 {NUM_GDDR} GDDR instances; valid bits are 0-{last}",
                last = NUM_GDDR - 1,
            );
            anyhow::ensure!(
                mask.count_ones() <= MAX_HARVESTED_DRAM,
                "{path}: 0x{mask:x} has {n} bits set, but only one GDDR instance can be \
                 soft-harvested at a time; use 0 (harvest none) or a single bit \
                 (1, 2, 4, 8, 16, 32, 64, 128)",
                n = mask.count_ones(),
            );
        }
        "product_spec_harvesting.dram_disable_count" => {
            let count = as_u32(path, value)?;
            // CalculateHarvesting computes `8 - count` into a uint8_t, so a
            // count above NUM_GDDR underflows to a huge value, the
            // POPCOUNT comparison never fires, and the product-spec harvest
            // is silently skipped altogether.
            anyhow::ensure!(
                count <= NUM_GDDR,
                "{path}: {count} exceeds the {NUM_GDDR} GDDR instances on this chip; \
                the allowable range of values for dram_disable_count are 0 or 1",
            );
            // Firmware clears a single bit (GDDR3) for this field no matter
            // how large the count is, so a count above 1 cannot do what it
            // says; worse, on a chip that already has one bank harvested by
            // fuses it pushes the total to two and UMD stops enumerating.
            anyhow::ensure!(
                count <= MAX_HARVESTED_DRAM,
                "{path}: {count} asks for more than {MAX_HARVESTED_DRAM} harvested DRAM \
                 bank, which firmware cannot honour; use 0 or 1 instead",
            );
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{field, Value};

    const MASK: &str = "dram_table.soft_harvest_dram_mask";
    const COUNT: &str = "product_spec_harvesting.dram_disable_count";

    /// Check a raw CLI value the way `Set::run` does. `table::set_value`
    /// parses numeric fields with this same `serde_json::Number` parse, so
    /// the value handed to `field` here matches the real one.
    fn check(path: &str, raw: &str) -> anyhow::Result<()> {
        let num = raw.parse().expect("test input parses as a JSON number");
        field(path, &Value::Number(num))
    }

    #[test]
    fn accepts_none_and_single_instance() {
        for raw in ["0", "1", "2", "4", "8", "16", "32", "64", "128"] {
            assert!(check(MASK, raw).is_ok(), "{raw} should be accepted");
        }
    }

    #[test]
    fn rejects_multiple_instances() {
        for raw in ["3", "5", "6", "9", "129", "255"] {
            let err = check(MASK, raw).expect_err("multi-bit mask should be rejected");
            assert!(
                err.to_string().contains("only one GDDR instance"),
                "unexpected error for {raw}: {err}"
            );
        }
    }

    #[test]
    fn rejects_bits_above_the_last_instance() {
        for raw in ["256", "512", "2147483648"] {
            let err = check(MASK, raw).expect_err("out-of-range mask should be rejected");
            assert!(
                err.to_string().contains("only 8 GDDR instances"),
                "unexpected error for {raw}: {err}"
            );
        }
    }

    #[test]
    fn accepts_harvest_counts_firmware_can_honour() {
        for raw in ["0", "1"] {
            assert!(check(COUNT, raw).is_ok(), "{raw} should be accepted");
        }
    }

    #[test]
    fn rejects_harvest_counts_above_one() {
        for raw in ["2", "3", "8"] {
            let err = check(COUNT, raw).expect_err("count above 1 should be rejected");
            assert!(
                err.to_string().contains("use 0 or 1"),
                "unexpected error for {raw}: {err}"
            );
        }
    }

    #[test]
    fn rejects_harvest_counts_that_underflow_firmware() {
        for raw in ["9", "100", "4294967295"] {
            let err = check(COUNT, raw).expect_err("count above 8 should be rejected");
            assert!(
                err.to_string().contains("exceeds the 8 GDDR instances"),
                "unexpected error for {raw}: {err}"
            );
        }
    }

    #[test]
    fn rejects_non_integer_values() {
        for path in [MASK, COUNT] {
            for raw in ["-1", "1.5", "4294967296"] {
                assert!(check(path, raw).is_err(), "{path}={raw} should be rejected");
            }
        }
    }

    #[test]
    fn leaves_other_fields_alone() {
        // tdp_limit is the other numeric field exposed for override and has
        // no bit-level constraint; dram_mask is the separate hard-disable
        // mask, which is not restricted to a single bit either. A value that
        // the harvesting fields would reject must pass for both.
        for path in ["chip_limits.tdp_limit", "dram_table.dram_mask"] {
            assert!(field(path, &Value::Number(255.into())).is_ok());
        }
    }
}
