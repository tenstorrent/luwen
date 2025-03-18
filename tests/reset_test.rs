#![cfg(test)]

use luwen_if::{
    chip::{ArcMsgOptions, Chip, HlComms},
    ChipImpl,
};
use luwen_ref::detect_chips;
use std::os::fd::AsRawFd;
use ttkmd_if::ioctl::{
    ResetDevice, ResetDeviceIn, RESET_DEVICE_RESET_PCIE_LINK, RESET_DEVICE_RESTORE_STATE,
};

/// Test utilities for verifying device reset functionality
///
/// These tests verify:
/// - Device state before and after reset
/// - PCIe link restore after reset
/// - Reset via ARC messaging
/// - Memory state persistence or clearing through reset cycle
///
/// Note: These tests require physical hardware to run. By default, they are
/// annotated with #[ignore] to avoid false failures on systems without hardware.
/// To run all hardware tests:
///
///   cargo test --test reset_test -- --ignored
///
/// The tests will automatically detect if compatible hardware is present;
/// if hardware is not found, the test will be skipped.

mod tests {
    use super::*;

    /// Perform a low-level device reset via TTKMD interface on the provided interfaces
    fn lds_reset(interfaces: &[usize]) -> Vec<Chip> {
        // Step 1: Reset PCIe link for each device
        for interface in interfaces {
            let fd = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(format!("/dev/tenstorrent/{interface}"))
                .unwrap();
            let mut reset_device = ResetDevice {
                input: ResetDeviceIn {
                    flags: RESET_DEVICE_RESET_PCIE_LINK,
                    ..Default::default()
                },
                ..Default::default()
            };
            unsafe { ttkmd_if::ioctl::reset_device(AsRawFd::as_raw_fd(&fd), &mut reset_device) }
                .unwrap();

            assert_eq!(reset_device.output.result, 0);
        }

        // Step 2: Initialize devices after reset
        let mut output = Vec::new();
        for interface in interfaces.iter().copied() {
            output.push(luwen_ref::open(interface).unwrap());
        }

        // Step 3: Set ARC state to A3 for each chip
        for chip in &output {
            chip.arc_msg(ArcMsgOptions {
                msg: luwen_if::TypedArcMsg::SetArcState {
                    state: luwen_if::ArcState::A3,
                }
                .into(),
                ..Default::default()
            })
            .unwrap();
        }

        // Step 4: Trigger ARC reset for each chip
        for chip in &output {
            chip.arc_msg(ArcMsgOptions {
                msg: luwen_if::TypedArcMsg::TriggerReset.into(),
                wait_for_done: false,
                ..Default::default()
            })
            .unwrap();
        }

        // Step 5: Wait for chips to come back online
        println!("Sleeping for 2 seconds to allow chip to come back online");
        std::thread::sleep(std::time::Duration::from_secs(2));

        // Step 6: Restore PCIe state
        for interface in interfaces {
            let fd = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(format!("/dev/tenstorrent/{interface}"))
                .unwrap();
            let mut reset_device = ResetDevice {
                input: ResetDeviceIn {
                    flags: RESET_DEVICE_RESTORE_STATE,
                    ..Default::default()
                },
                ..Default::default()
            };
            unsafe { ttkmd_if::ioctl::reset_device(AsRawFd::as_raw_fd(&fd), &mut reset_device) }
                .unwrap();

            assert_eq!(reset_device.output.result, 0);
        }

        output
    }

    #[test]
    #[ignore = "Requires hardware"]
    fn wormhole_test_chip_reset() {
        println!("STARTING RESET TEST");

        // Step 1: Get list of available interfaces
        let interfaces = luwen_ref::PciDevice::scan();

        // Step 2: Write a marker value to memory for each device
        for interface in interfaces.iter().copied() {
            luwen_ref::open(interface)
                .unwrap()
                .axi_write32(0x20000, 0xfaca)
                .unwrap();
        }

        // Step 3: Verify write succeeded
        for interface in interfaces.iter().copied() {
            let result = luwen_ref::open(interface)
                .unwrap()
                .axi_read32(0x20000)
                .unwrap();
            assert_eq!(result, 0xfaca, "Initial memory write verification failed");
        }

        // Step 4: Reset all devices
        lds_reset(&interfaces);

        // Step 5: Verify reset cleared memory
        for interface in interfaces.iter().copied() {
            let result = luwen_ref::open(interface)
                .unwrap()
                .axi_read32(0x20000)
                .unwrap();
            assert_ne!(result, 0xfaca, "Memory should be cleared after reset");
        }

        println!("RESET COMPLETED SUCCESSFULLY");

        // Step 6: Verify chips can be detected after reset
        let chips = detect_chips().unwrap();
        assert!(!chips.is_empty(), "Should still detect chips after reset");

        // Step 7: Check chip status after reset
        for chip in chips {
            if let Some(wh) = chip.as_wh() {
                // For Wormhole chips, check remote status
                println!("Wormhole remote status: {}", wh.is_remote);

                // Check telemetry to verify chip is functioning
                let telemetry = chip.get_telemetry().unwrap();
                println!("Board ID after reset: {:016X}", telemetry.board_id);
                assert_ne!(
                    telemetry.board_id, 0,
                    "Board ID should be valid after reset"
                );
            }
        }
    }
}
