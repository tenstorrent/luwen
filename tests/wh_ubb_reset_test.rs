#![cfg(test)]

use serial_test::serial;

use luwen::api::chip::wh_ubb;

/// Test utilities for verifying ubb reset ability
///
/// These tests verify:
/// - UBB reset functionality
///
/// Note: These tests require physical hardware to run. By default, they are
/// annotated with #[ignore] to avoid false failures on systems without hardware.
/// To run all hardware tests:
///
///   cargo test --test telem_test -- --ignored
///
/// The tests will automatically detect if compatible hardware is present;
/// if hardware is not found, the test will be skipped.
mod test_utils;

#[serial]
mod tests {
    use super::*;

    #[test]
    #[cfg_attr(
        not(all(feature = "test_hardware", feature = "test_wh_ubb")),
        ignore = "Requires real wormhole hardware"
    )]
    fn wh_ubb_reset_test() {
        wh_ubb::wh_ubb_ipmi_reset("0xF", "0xFF", "0x0", "0xF")
            .expect("Failed to execute wh_ubb_ipmi_reset");
    }
}
