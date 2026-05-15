use std::os::unix::fs::FileExt;

pub fn chip_reset(interface_id: usize) -> Result<(), Box<dyn std::error::Error>> {
    let device = luwen::kmd::PciDevice::open(interface_id)?;
    let p = &device.physical;
    let bdf = format!(
        "{:04x}:{:02x}:{:02x}.{:x}",
        p.pci_domain, p.pci_bus, p.slot, p.pci_function
    );

    // Initiate reset
    {
        let fd = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(format!("/dev/tenstorrent/{interface_id}"))?;
        let mut req = luwen::kmd::ioctl::ResetDevice {
            input: luwen::kmd::ioctl::ResetDeviceIn {
                flags: luwen::kmd::ioctl::RESET_DEVICE_RESET_CONFIG_WRITE,
                ..Default::default()
            },
            ..Default::default()
        };
        unsafe {
            luwen::kmd::ioctl::reset_device(
                std::os::fd::AsRawFd::as_raw_fd(&fd),
                std::ptr::addr_of_mut!(req),
            )
        }?;
        assert_eq!(req.output.result, 0);
    }

    // Wait for reset to assert then clear (2-second timeout)
    let start = std::time::Instant::now();
    let mut saw_in_reset = false;
    while start.elapsed().as_secs() < 2 {
        if let Ok(file) = std::fs::OpenOptions::new()
            .read(true)
            .open(format!("/sys/bus/pci/devices/{bdf}/config"))
        {
            let mut buf = [0u8; 1];
            if file.read_exact_at(&mut buf, 4).is_ok() {
                let in_reset = (buf[0] >> 1) & 1 != 0;
                if !saw_in_reset {
                    if in_reset {
                        saw_in_reset = true;
                    }
                } else if !in_reset {
                    break;
                }
            }
        }
    }

    // Restore
    {
        let fd = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(format!("/dev/tenstorrent/{interface_id}"))?;
        let mut req = luwen::kmd::ioctl::ResetDevice {
            input: luwen::kmd::ioctl::ResetDeviceIn {
                flags: luwen::kmd::ioctl::RESET_DEVICE_RESTORE_STATE,
                ..Default::default()
            },
            ..Default::default()
        };
        unsafe {
            luwen::kmd::ioctl::reset_device(
                std::os::fd::AsRawFd::as_raw_fd(&fd),
                std::ptr::addr_of_mut!(req),
            )
        }?;
        assert_eq!(req.output.result, 0);
    }

    Ok(())
}
