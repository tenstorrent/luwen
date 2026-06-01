use std::os::unix::fs::FileExt;
use std::path::Path;

/// `TENSTORRENT_RESET_DEVICE_ASIC_RESET` — full ASIC reset (clocks, NOC,
/// Tensix array, DRAM, ETH). Drops the `PCIe` link; device may disappear
/// and reappear on the bus.
const RESET_DEVICE_ASIC_RESET: u32 = 4;
/// `TENSTORRENT_RESET_DEVICE_POST_RESET` — post-reset bringup (restore PCI
/// config, re-init device). Issued after the ASIC has come back.
const RESET_DEVICE_POST_RESET: u32 = 6;

pub fn chip_reset(interface_id: usize) -> Result<(), Box<dyn std::error::Error>> {
    let dev_path = format!("/dev/tenstorrent/{interface_id}");
    if !Path::new(&dev_path).exists() {
        // Chip detected via Ethernet, not direct PCIe — there's no kmd device
        // node to reset. The reset on the PCIe-attached gateway chip will
        // cascade to its ETH-routed peers.
        tracing::debug!(interface_id, "skipping reset: no kmd device node");
        return Ok(());
    }

    let device = luwen::kmd::PciDevice::open(interface_id)?;
    let p = &device.physical;
    let bdf = format!(
        "{:04x}:{:02x}:{:02x}.{:x}",
        p.pci_domain, p.pci_bus, p.slot, p.pci_function
    );

    tracing::info!(interface_id, %bdf, "issuing ASIC_RESET");
    reset_ioctl(&dev_path, RESET_DEVICE_ASIC_RESET)?;

    // Wait for reset to assert then clear (2-second timeout).
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

    tracing::info!(interface_id, %bdf, "issuing POST_RESET");
    reset_ioctl(&dev_path, RESET_DEVICE_POST_RESET)?;

    Ok(())
}

fn reset_ioctl(dev_path: &str, flags: u32) -> Result<(), Box<dyn std::error::Error>> {
    let fd = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(dev_path)?;
    let mut req = luwen::kmd::ioctl::ResetDevice {
        input: luwen::kmd::ioctl::ResetDeviceIn {
            flags,
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
    Ok(())
}
