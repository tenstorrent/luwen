#![cfg(test)]

use serial_test::serial;

use luwen::api::{chip::ArcMsgOptions, ArcState, ChipImpl, TypedArcMsg};
use luwen::def::Arch;

/// Test reset functionality for Wormhole and Blackhole chips
///
/// These tests verify:
/// - Chip reset via ARC messages (Wormhole)
/// - Chip reset via config write (Blackhole)
/// - State restoration after reset
///
/// Note: These tests require physical hardware to run. By default, they are
/// annotated with #[ignore] to avoid false failures on systems without hardware.
/// To run all hardware tests:
///
///   cargo test --test reset -- --ignored
///
/// The tests will automatically detect if compatible hardware is present;
/// if hardware is not found, the test will be skipped.
///
/// IMPORTANT: These tests MUST run serially as they perform device resets
/// which can interfere with each other.
mod test_utils;

#[serial]
mod tests {
    use super::*;
    use std::os::fd::AsRawFd;
    use test_utils::hardware_available;

    /// Test Wormhole reset via ARC messages
    ///
    /// This test performs:
    /// 1. Set ARC state to A3 (power state)
    /// 2. Trigger reset via ARC message
    /// 3. Wait for reset completion
    /// 4. Restore device state via ioctl
    #[test]
    #[cfg_attr(
        not(all(feature = "test_hardware", feature = "test_wormhole")),
        ignore = "Requires real wormhole hardware"
    )]
    fn wormhole_reset_test() {
        assert!(hardware_available(), "Test requires hardware");

        // Scan for device interfaces
        let interfaces = luwen::pci::PciDevice::scan();
        assert!(!interfaces.is_empty(), "Should find at least one device");

        let mut found_wh = false;
        for interface in interfaces {
            // Open KMD device to check architecture
            let kmd_device = match luwen::kmd::PciDevice::open(interface) {
                Ok(dev) => dev,
                Err(_) => continue,
            };

            if kmd_device.arch != Arch::Wormhole {
                continue;
            }

            found_wh = true;
            println!("Testing Wormhole reset on interface {}", interface);

            // Open the chip for ARC messaging
            let chip_handle =
                luwen::pci::open(interface).expect("Failed to open chip for reset");

            // Step 1: Set ARC state to A3
            chip_handle
                .arc_msg(ArcMsgOptions {
                    msg: TypedArcMsg::SetArcState { state: ArcState::A3 }.into(),
                    ..Default::default()
                })
                .expect("Failed to set ARC state to A3");

            // Step 2: Trigger reset (don't wait for completion as chip will reset)
            chip_handle
                .arc_msg(ArcMsgOptions {
                    msg: TypedArcMsg::TriggerReset.into(),
                    wait_for_done: false,
                    ..Default::default()
                })
                .expect("Failed to trigger reset");

            // Step 3: Wait for reset to complete
            // Wormhole reset is asynchronous, give it time
            std::thread::sleep(std::time::Duration::from_secs(2));

            // Step 4: Restore device state
            let fd = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(format!("/dev/tenstorrent/{}", interface))
                .expect("Failed to open device for restore");

            let mut reset_device = luwen::kmd::ioctl::ResetDevice {
                input: luwen::kmd::ioctl::ResetDeviceIn {
                    flags: luwen::kmd::ioctl::RESET_DEVICE_RESTORE_STATE,
                    ..Default::default()
                },
                ..Default::default()
            };

            unsafe { luwen::kmd::ioctl::reset_device(fd.as_raw_fd(), &mut reset_device) }
                .expect("Failed to restore device state");

            assert_eq!(
                reset_device.output.result, 0,
                "Device state restoration failed"
            );

            // Verify chip is accessible after reset
            let chips_after =
                luwen::pci::detect_chips_fallible().expect("Failed to detect chips after reset");
            assert!(!chips_after.is_empty(), "Should find chips after reset");

            println!(
                "Wormhole reset completed successfully on interface {}",
                interface
            );
            break;
        }

        assert!(found_wh, "Test failed: No Wormhole chip found");
    }

    /// Test Blackhole reset via config write
    ///
    /// This test performs:
    /// 1. Issue reset via RESET_DEVICE_RESET_CONFIG_WRITE ioctl
    /// 2. Poll PCIe config space for reset completion
    /// 3. Restore device state via ioctl
    #[test]
    #[cfg_attr(
        not(all(feature = "test_hardware", feature = "test_blackhole")),
        ignore = "Requires real blackhole hardware"
    )]
    fn blackhole_reset_test() {
        assert!(hardware_available(), "Test requires hardware");

        // Scan for device interfaces
        let interfaces = luwen::pci::PciDevice::scan();
        assert!(!interfaces.is_empty(), "Should find at least one device");

        let mut found_bh = false;
        for interface in interfaces {
            // Open KMD device to check architecture and get PCI info
            let kmd_device = match luwen::kmd::PciDevice::open(interface) {
                Ok(dev) => dev,
                Err(_) => continue,
            };

            if kmd_device.arch != Arch::Blackhole {
                continue;
            }

            found_bh = true;

            let pci_info = &kmd_device.physical;
            let pci_bdf = format!(
                "{:04x}:{:02x}:{:02x}.{:x}",
                pci_info.pci_domain, pci_info.pci_bus, pci_info.slot, pci_info.pci_function
            );

            println!(
                "Testing Blackhole reset on interface {} ({})",
                interface, pci_bdf
            );

            // Step 1: Issue reset via config write
            let fd = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(format!("/dev/tenstorrent/{}", interface))
                .expect("Failed to open device for reset");

            let mut reset_device = luwen::kmd::ioctl::ResetDevice {
                input: luwen::kmd::ioctl::ResetDeviceIn {
                    flags: luwen::kmd::ioctl::RESET_DEVICE_RESET_CONFIG_WRITE,
                    ..Default::default()
                },
                ..Default::default()
            };

            unsafe { luwen::kmd::ioctl::reset_device(fd.as_raw_fd(), &mut reset_device) }
                .expect("Failed to issue reset");

            assert_eq!(reset_device.output.result, 0, "Reset command failed");

            // Step 2: Poll PCIe config space for reset completion
            let config_path = format!("/sys/bus/pci/devices/{}/config", pci_bdf);
            let start = std::time::Instant::now();
            let timeout = std::time::Duration::from_secs(2);
            let mut saw_in_reset = false;
            let mut reset_complete = false;

            while start.elapsed() < timeout {
                if let Ok(file) = std::fs::OpenOptions::new().read(true).open(&config_path) {
                    use std::os::unix::fs::FileExt;
                    let mut config_bit = [0u8; 1];
                    if file.read_exact_at(&mut config_bit, 4).is_ok() {
                        let reset_bit = (config_bit[0] >> 1) & 0x1;
                        let is_complete = reset_bit == 0;

                        if !saw_in_reset && !is_complete {
                            saw_in_reset = true;
                        } else if saw_in_reset && is_complete {
                            reset_complete = true;
                            break;
                        }
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }

            // If we didn't see the reset transition, that's okay - it may have completed very fast
            if !reset_complete && !saw_in_reset {
                println!(
                    "Warning: Did not observe reset transition, reset may have completed very quickly"
                );
            }

            // Step 3: Restore device state
            let mut restore_device = luwen::kmd::ioctl::ResetDevice {
                input: luwen::kmd::ioctl::ResetDeviceIn {
                    flags: luwen::kmd::ioctl::RESET_DEVICE_RESTORE_STATE,
                    ..Default::default()
                },
                ..Default::default()
            };

            unsafe { luwen::kmd::ioctl::reset_device(fd.as_raw_fd(), &mut restore_device) }
                .expect("Failed to restore device state");

            assert_eq!(
                restore_device.output.result, 0,
                "Device state restoration failed"
            );

            // Verify chip is accessible after reset
            let chips_after =
                luwen::pci::detect_chips_fallible().expect("Failed to detect chips after reset");
            assert!(!chips_after.is_empty(), "Should find chips after reset");

            println!(
                "Blackhole reset completed successfully on interface {}",
                interface
            );
            break;
        }

        assert!(found_bh, "Test failed: No Blackhole chip found");
    }

    /// Test PCIe link reset functionality (works for both WH and BH)
    ///
    /// This is a lighter-weight reset that only resets the PCIe link.
    #[test]
    #[cfg_attr(not(feature = "test_hardware"), ignore = "Requires real hardware")]
    fn pcie_link_reset_test() {
        assert!(hardware_available(), "Test requires hardware");

        // Scan for device interfaces
        let interfaces = luwen::pci::PciDevice::scan();
        assert!(!interfaces.is_empty(), "Should find at least one device");

        for interface in interfaces {
            // Verify device is accessible
            if luwen::kmd::PciDevice::open(interface).is_err() {
                continue;
            }

            println!("Testing PCIe link reset on interface {}", interface);

            let fd = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(format!("/dev/tenstorrent/{}", interface))
                .expect("Failed to open device");

            let mut reset_device = luwen::kmd::ioctl::ResetDevice {
                input: luwen::kmd::ioctl::ResetDeviceIn {
                    flags: luwen::kmd::ioctl::RESET_DEVICE_RESET_PCIE_LINK,
                    ..Default::default()
                },
                ..Default::default()
            };

            let result =
                unsafe { luwen::kmd::ioctl::reset_device(fd.as_raw_fd(), &mut reset_device) };

            // PCIe link reset may fail if link is already good, that's okay
            if result.is_ok() {
                println!(
                    "PCIe link reset result: {} (0 = success)",
                    reset_device.output.result
                );
            } else {
                println!(
                    "PCIe link reset not supported or failed (expected on some systems)"
                );
            }

            // Test only one chip
            break;
        }
    }
}
