#![cfg(test)]

use luwen_core::Arch;
use luwen_if::{chip::HlCommsInterface, ChipImpl};

/// Test utilities for verifying telemetry functionality
///
/// These tests verify:
/// - Chip telemetry collection
/// - Chip status reporting
/// - Chip architecture detection
/// - Chip-specific functionality (Wormhole, Grayskull, Blackhole)
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

mod tests {
    use super::*;

    #[test]
    #[cfg_attr(
        not(all(feature = "test_hardware", feature = "test_wormhole")),
        ignore = "Requires real wormhole hardware"
    )]
    fn wormhole_test_chip_telemetry() {
        let partial_chips = luwen_ref::detect_chips_fallible().unwrap();
        assert!(!partial_chips.is_empty(), "Should find at least one chip");

        for chip in partial_chips {
            let upgraded_chip = chip.try_upgrade();
            if let Some(upgraded_chip) = upgraded_chip {
                // Only test Wormhole chips
                if let Some(wh) = upgraded_chip.as_wh() {
                    let status = chip.status();
                    println!("Wormhole chip status: {:?}", status);

                    let eth_status = chip.eth_safe();
                    println!("Wormhole ethernet status: {:?}", eth_status);

                    println!("Testing Wormhole chip");

                    if chip.arc_alive() {
                        let telemetry = wh.get_telemetry().unwrap();
                        println!("Wormhole board ID: {:X}", telemetry.board_id_low);
                        assert_ne!(telemetry.board_id_low, 0, "Board ID should be non-zero");
                    }

                    // Check remote status
                    println!("Wormhole remote status: {}", wh.is_remote);

                    // Print chip information
                    println!(
                        "Wormhole chip: {:?}, Status: {:?}, Ethernet: {:?}",
                        upgraded_chip.get_arch(),
                        status,
                        eth_status
                    );

                    // Verify that architecture is reported correctly
                    assert_eq!(
                        upgraded_chip.get_arch(),
                        Arch::Wormhole,
                        "Architecture should be Wormhole"
                    );
                }
            }
        }
    }

    #[test]
    #[cfg_attr(
        not(all(feature = "test_hardware", feature = "test_grayskull")),
        ignore = "Requires real grayskull hardware"
    )]
    fn grayskull_test_chip_telemetry() {
        let partial_chips = luwen_ref::detect_chips_fallible().unwrap();
        assert!(!partial_chips.is_empty(), "Should find at least one chip");

        for chip in partial_chips {
            let upgraded_chip = chip.try_upgrade();
            if let Some(upgraded_chip) = upgraded_chip {
                // Only test Grayskull chips
                if let Some(gs) = upgraded_chip.as_gs() {
                    let status = chip.status();
                    println!("Grayskull chip status: {:?}", status);

                    let eth_status = chip.eth_safe();
                    println!("Grayskull ethernet status: {:?}", eth_status);

                    println!("Testing Grayskull chip");

                    // Read scratch register
                    let scratch_value = gs.axi_sread32("ARC_RESET.SCRATCH[0]").unwrap();
                    println!("Grayskull scratch value: {:x}", scratch_value);

                    // Print chip information
                    println!(
                        "Grayskull chip: {:?}, Status: {:?}, Ethernet: {:?}",
                        upgraded_chip.get_arch(),
                        status,
                        eth_status
                    );

                    // Verify that architecture is reported correctly
                    assert_eq!(
                        upgraded_chip.get_arch(),
                        Arch::Grayskull,
                        "Architecture should be Grayskull"
                    );
                }
            }
        }
    }

    #[test]
    #[cfg_attr(
        not(all(feature = "test_hardware", feature = "test_blackhole")),
        ignore = "Requires real blackhole hardware"
    )]
    fn blackhole_test_chip_telemetry() {
        let partial_chips = luwen_ref::detect_chips_fallible().unwrap();
        assert!(!partial_chips.is_empty(), "Should find at least one chip");

        for chip in partial_chips {
            let upgraded_chip = chip.try_upgrade();
            if let Some(upgraded_chip) = upgraded_chip {
                // Only test Blackhole chips
                if let Some(bh) = upgraded_chip.as_bh() {
                    let status = chip.status();
                    println!("Blackhole chip status: {:?}", status);

                    let eth_status = chip.eth_safe();
                    println!("Blackhole ethernet status: {:?}", eth_status);

                    println!("Testing Blackhole chip");

                    // Get telemetry twice to verify consistency
                    let telemetry1 = bh.get_telemetry().unwrap();
                    let telemetry2 = bh.get_telemetry().unwrap();

                    println!("Blackhole telemetry: {:?}", telemetry1);
                    println!("Blackhole telemetry: {:?}", telemetry2);

                    // Get subsystem ID
                    if let Some(subsystem) = bh.get_if::<luwen_if::chip::NocInterface>()
                        .map(|v| &v.backing)
                        .and_then(|v| {
                            v.as_any()
                                .downcast_ref::<luwen_if::CallbackStorage<luwen_ref::ExtendedPciDeviceWrapper>>()
                        })
                        .map(|v| v.user_data.borrow().device.physical.subsystem_id) {
                        println!("Blackhole subsystem ID: {:x}", subsystem);
                        assert_ne!(subsystem, 0, "Subsystem ID should be non-zero");
                    }

                    // Print chip information
                    println!(
                        "Blackhole chip: {:?}, Status: {:?}, Ethernet: {:?}",
                        upgraded_chip.get_arch(),
                        status,
                        eth_status
                    );

                    // Verify that architecture is reported correctly
                    assert_eq!(
                        upgraded_chip.get_arch(),
                        Arch::Blackhole,
                        "Architecture should be Blackhole"
                    );
                }
            }
        }
    }
}
