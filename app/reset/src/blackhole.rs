use std::{fs::OpenOptions, os::unix::fs::FileExt};

use crate::Reset;

pub struct ResetTracker {
    pci_bdf: String,
    interface: usize,
    saw_in_reset: bool,
}

impl ResetTracker {
    pub fn init(interface: usize) -> Self {
        let device = ttkmd_if::PciDevice::open(interface).unwrap();

        let info = &device.physical;
        Self {
            pci_bdf: format!(
                "{:04x}:{:02x}:{:02x}.{:x}",
                info.pci_domain, info.pci_bus, info.slot, info.pci_function
            ),
            interface,
            saw_in_reset: false,
        }
    }
}

impl Reset for ResetTracker {
    fn reset(&mut self) {
        let fd = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(format!("/dev/tenstorrent/{}", self.interface))
            .unwrap();
        let mut reset_device = ttkmd_if::ioctl::ResetDevice {
            input: ttkmd_if::ioctl::ResetDeviceIn {
                flags: ttkmd_if::ioctl::RESET_DEVICE_RESET_CONFIG_WRITE,
                ..Default::default()
            },
            ..Default::default()
        };
        unsafe {
            ttkmd_if::ioctl::reset_device(std::os::fd::AsRawFd::as_raw_fd(&fd), &mut reset_device)
        }
        .unwrap();

        assert_eq!(reset_device.output.result, 0);
    }

    fn wait(&mut self) -> bool {
        if let Ok(file) = OpenOptions::new()
            .read(true)
            .open(format!("/sys/bus/pci/devices/{}/config", self.pci_bdf))
        {
            let mut config_bit = [0; 1];
            file.read_exact_at(&mut config_bit, 4).unwrap();
            let config_bit = config_bit[0];
            let reset_bit = (config_bit >> 1) & 0x1;
            let reset_complete = reset_bit == 0;

            if !self.saw_in_reset {
                if !reset_complete {
                    self.saw_in_reset = true;
                }
            } else if reset_complete {
                return true;
            }
        }

        false
    }

    fn restore(&mut self) {
        let fd = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(format!("/dev/tenstorrent/{}", self.interface))
            .unwrap();
        let mut reset_device = ttkmd_if::ioctl::ResetDevice {
            input: ttkmd_if::ioctl::ResetDeviceIn {
                flags: ttkmd_if::ioctl::RESET_DEVICE_RESTORE_STATE,
                ..Default::default()
            },
            ..Default::default()
        };
        unsafe {
            ttkmd_if::ioctl::reset_device(std::os::fd::AsRawFd::as_raw_fd(&fd), &mut reset_device)
        }
        .unwrap();

        assert_eq!(reset_device.output.result, 0);
    }
}
