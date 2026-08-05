//! Value checks for `bh-mod set`.
//!
//! `fw_table_override.proto` types its fields as bare integers, so the
//! schema alone permits values that leave the chip misconfigured or
//! undetectable by the host. These checks run before anything is staged for
//! writing, so a rejected value leaves flash and the chip untouched.

use anyhow::Context as _;
use serde_json::Value;

/// Dot-path of the soft-harvest mask. `table::Set` matches on it to decide
/// whether it needs to read the chip.
pub const SOFT_HARVEST_DRAM_MASK: &str = "dram_table.soft_harvest_dram_mask";

/// GDDR instances on a Blackhole chip; the mask carries one bit each.
const NUM_GDDR: u32 = 8;

/// Bits 0..`NUM_GDDR` — the window every GDDR bitmap is defined over.
const GDDR_MASK: u32 = (1 << NUM_GDDR) - 1;

/// DRAM banks that may be harvested at once. Messages below assume 1.
const MAX_HARVESTED_DRAM: u32 = 1;

/// Chip state the checks need. The default means "not read", which skips
/// the checks that use it; `table::Set` reads the chip only when a field
/// that needs it is being assigned.
#[derive(Default)]
pub struct State {
    /// Harvested GDDR instances that the active mask does not account for.
    /// `None` when not read.
    pub unaccounted_harvest: Option<u32>,
}

impl State {
    /// Build the state from a chip read, for passing to [`field`].
    ///
    /// `enabled_gddr` is telemetry tag 36, in which a set bit means the
    /// instance survived; `soft_harvested` is the mask in effect right now.
    /// Inverting the first gives every harvested instance, and removing the
    /// second leaves the ones the user cannot undo — so replacing a mask you
    /// set yourself stays allowed.
    ///
    /// A zero `enabled_gddr` means the chip never reported the tag, and
    /// yields the default so the check is skipped.
    pub fn from_telemetry(enabled_gddr: u32, soft_harvested: u32) -> Self {
        if enabled_gddr & GDDR_MASK == 0 {
            tracing::warn!("telemetry reports no enabled GDDR; skipping harvest cross-check");
            return Self::default();
        }
        let harvested = !enabled_gddr & GDDR_MASK;
        Self {
            unaccounted_harvest: Some(harvested & !soft_harvested & GDDR_MASK),
        }
    }
}

/// Format a GDDR bitmap as instance numbers: `0b101` becomes `0, 2`.
fn instance_list(mask: u32) -> String {
    (0..NUM_GDDR)
        .filter(|i| (mask >> i) & 1 == 1)
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// The values a single-instance mask may take: `1, 2, 4, ...`.
fn single_instance_values() -> String {
    (0..NUM_GDDR)
        .map(|i| (1u32 << i).to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Read a field's value as the unsigned integer its proto type promises.
fn as_u32(path: &str, value: &Value) -> anyhow::Result<u32> {
    value
        .as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .with_context(|| format!("{path} must be a 32-bit non-negative integer"))
}

/// Check one `field=value` assignment, called by `table::Set` before the
/// value is staged. Paths with no constraints pass unchanged.
pub fn field(path: &str, value: &Value, state: &State) -> anyhow::Result<()> {
    if path == SOFT_HARVEST_DRAM_MASK {
        let mask = as_u32(path, value)?;
        anyhow::ensure!(
            mask & !GDDR_MASK == 0,
            "{path}: {mask} is out of range; use 0 or one of {opts}",
            opts = single_instance_values(),
        );
        anyhow::ensure!(
            mask.count_ones() <= MAX_HARVESTED_DRAM,
            "{path}: {mask} selects {n} GDDR instances but only one can be \
             harvested; use 0 or one of {opts}",
            n = mask.count_ones(),
            opts = single_instance_values(),
        );
        // Only the union matters: naming an instance that is already down
        // costs nothing, adding a second one does not work.
        if let Some(already) = state.unaccounted_harvest {
            anyhow::ensure!(
                (already | mask).count_ones() <= MAX_HARVESTED_DRAM,
                "{path}: GDDR {have} is already harvested on this chip, so GDDR \
                 {want} cannot be harvested as well; clear this override with \
                 `bh-mod res {path}`",
                have = instance_list(already),
                want = instance_list(mask & !already),
            );
            if mask & already != 0 {
                tracing::warn!(
                    "{path}: GDDR {list} is already harvested on this chip, so this \
                     override has no effect",
                    list = instance_list(mask & already),
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{field, State, Value, GDDR_MASK, SOFT_HARVEST_DRAM_MASK as MASK};

    /// Every instance enabled — a part with nothing harvested.
    const ALL_ENABLED: u32 = GDDR_MASK;

    /// Check a raw CLI value the way `Set::run` does. `table::set_value`
    /// parses numeric fields with this same `serde_json::Number` parse.
    fn check(path: &str, raw: &str, state: &State) -> anyhow::Result<()> {
        let num = raw.parse().expect("test input parses as a JSON number");
        field(path, &Value::Number(num), state)
    }

    #[test]
    fn accepts_none_and_single_instance() {
        for raw in ["0", "1", "2", "4", "8", "16", "32", "64", "128"] {
            assert!(
                check(MASK, raw, &State::default()).is_ok(),
                "{raw} should be accepted"
            );
        }
    }

    #[test]
    fn rejects_multiple_instances() {
        for raw in ["3", "5", "6", "9", "129", "255"] {
            let err =
                check(MASK, raw, &State::default()).expect_err("multi-bit mask should be rejected");
            assert!(
                err.to_string().contains("only one can be harvested"),
                "unexpected error for {raw}: {err}"
            );
        }
    }

    #[test]
    fn rejects_bits_above_the_last_instance() {
        for raw in ["256", "512", "2147483648"] {
            let err = check(MASK, raw, &State::default())
                .expect_err("out-of-range mask should be rejected");
            assert!(
                err.to_string().contains("is out of range"),
                "unexpected error for {raw}: {err}"
            );
        }
    }

    #[test]
    fn rejects_non_integer_values() {
        for raw in ["-1", "1.5", "4294967296"] {
            assert!(
                check(MASK, raw, &State::default()).is_err(),
                "{raw} should be rejected"
            );
        }
    }

    #[test]
    fn accepts_single_instance_on_an_unharvested_part() {
        let state = State::from_telemetry(ALL_ENABLED, 0);
        for raw in ["0", "1", "8", "128"] {
            assert!(check(MASK, raw, &state).is_ok(), "{raw} should be accepted");
        }
    }

    #[test]
    fn rejects_harvesting_a_second_bank() {
        let state = State::from_telemetry(ALL_ENABLED & !(1 << 2), 0);
        for raw in ["1", "8", "128"] {
            let err = check(MASK, raw, &state).expect_err("a second bank must not be harvested");
            assert!(
                err.to_string().contains("GDDR 2 is already harvested"),
                "unexpected error for {raw}: {err}"
            );
        }
    }

    #[test]
    fn accepts_a_mask_that_lands_on_the_harvested_instance() {
        // Naming an instance that is already down is a no-op, not a second
        // bank; clearing to 0 is likewise harmless.
        let state = State::from_telemetry(ALL_ENABLED & !(1 << 2), 0);
        assert_eq!(state.unaccounted_harvest, Some(1 << 2));
        assert!(check(MASK, "4", &state).is_ok());
        assert!(check(MASK, "0", &state).is_ok());
    }

    #[test]
    fn replacing_an_existing_mask_stays_allowed() {
        // GDDR3 is down only because the active mask asked for it.
        let state = State::from_telemetry(ALL_ENABLED & !(1 << 3), 8);
        assert!(check(MASK, "16", &state).is_ok());
        assert!(check(MASK, "0", &state).is_ok());
    }

    #[test]
    fn unreported_telemetry_does_not_block_writes() {
        assert_eq!(State::from_telemetry(0, 0).unaccounted_harvest, None);
        assert!(check(MASK, "8", &State::from_telemetry(0, 0)).is_ok());
    }

    #[test]
    fn harvest_arithmetic_ignores_bits_above_the_last_instance() {
        assert_eq!(
            State::from_telemetry(ALL_ENABLED, 0).unaccounted_harvest,
            Some(0)
        );
        assert_eq!(
            State::from_telemetry(!0xf0, 0).unaccounted_harvest,
            Some(0xf0)
        );
    }

    #[test]
    fn leaves_other_fields_alone() {
        for path in ["chip_limits.tdp_limit", "dram_table.dram_mask"] {
            assert!(field(path, &Value::Number(255.into()), &State::default()).is_ok());
        }
    }
}
