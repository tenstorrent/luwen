// SPDX-FileCopyrightText: © 2024 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

//! Blackhole-specific reset implementation.

use std::fs::OpenOptions;
use std::os::fd::AsRawFd;
use std::os::unix::fs::FileExt;

use crate::kmd::ioctl::{self, ResetDevice, ResetDeviceIn, RESET_DEVICE_RESET_CONFIG_WRITE};
use crate::kmd::PciDevice as KmdPciDevice;

use super::{restore_device_state, ResetError};

/// Blackhole-specific reset tracker.
///
/// This implementation uses `ioctl` calls to reset Blackhole chips and monitors
/// the PCI config space to detect reset completion.
pub struct Reset {
    interface: usize,
    pci_bdf: String,
    saw_in_reset: bool,
}

impl Reset {
    /// Creates a new Blackhole reset tracker for the given interface.
    ///
    /// # Errors
    ///
    /// Returns an error if the PCI device cannot be opened.
    pub fn new(interface: usize) -> Result<Self, ResetError> {
        let device = KmdPciDevice::open(interface).map_err(|e| ResetError::ChipOpenFailed {
            interface,
            message: format!("{:?}", e),
        })?;

        let info = &device.physical;
        let pci_bdf = format!(
            "{:04x}:{:02x}:{:02x}.{:x}",
            info.pci_domain, info.pci_bus, info.slot, info.pci_function
        );

        Ok(Self {
            interface,
            pci_bdf,
            saw_in_reset: false,
        })
    }
}

impl super::Reset for Reset {
    fn interface(&self) -> usize {
        self.interface
    }

    fn initiate_reset(&mut self) -> Result<(), ResetError> {
        let fd = OpenOptions::new()
            .read(true)
            .write(true)
            .open(format!("/dev/tenstorrent/{}", self.interface))
            .map_err(|e| ResetError::DeviceOpenFailed {
                interface: self.interface,
                source: e,
            })?;

        let mut reset_dev = ResetDevice {
            input: ResetDeviceIn {
                flags: RESET_DEVICE_RESET_CONFIG_WRITE,
                ..Default::default()
            },
            ..Default::default()
        };

        unsafe { ioctl::reset_device(fd.as_raw_fd(), &mut reset_dev) }.map_err(|e| {
            ResetError::IoctlFailed {
                interface: self.interface,
                message: format!("{:?}", e),
            }
        })?;

        if reset_dev.output.result != 0 {
            return Err(ResetError::IoctlFailed {
                interface: self.interface,
                message: format!("ioctl returned error code {}", reset_dev.output.result),
            });
        }

        Ok(())
    }

    fn poll_reset_complete(&mut self) -> Result<bool, ResetError> {
        // Read PCI config space to check reset status.
        let config_path = format!("/sys/bus/pci/devices/{}/config", self.pci_bdf);
        if let Ok(file) = OpenOptions::new().read(true).open(&config_path) {
            let mut config_bit = [0u8; 1];
            if file.read_exact_at(&mut config_bit, 4).is_ok() {
                let reset_bit = (config_bit[0] >> 1) & 0x1;
                let reset_complete = reset_bit == 0;

                if !self.saw_in_reset {
                    if !reset_complete {
                        self.saw_in_reset = true;
                    }
                } else if reset_complete {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    fn restore_state(&mut self) -> Result<(), ResetError> {
        restore_device_state(self.interface)
    }
}
