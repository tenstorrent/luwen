use std::path::Path;
use std::time::{Duration, Instant};

/// `TENSTORRENT_RESET_DEVICE_ASIC_RESET` — full ASIC reset including a
/// `PCIe` link drop; the device disappears and re-enumerates with a
/// possibly different `/dev/tenstorrent/<id>`.
const RESET_DEVICE_ASIC_RESET: u32 = 4;
/// `TENSTORRENT_RESET_DEVICE_POST_RESET` — issued after the device has
/// reappeared. Restores PCI config and re-inits the kmd state.
const RESET_DEVICE_POST_RESET: u32 = 6;

pub fn chip_reset(interface_id: usize) -> Result<(), Box<dyn std::error::Error>> {
    let dev_path = format!("/dev/tenstorrent/{interface_id}");
    if !Path::new(&dev_path).exists() {
        // Chip detected via Ethernet, not direct PCIe — there's no kmd
        // device node to reset. The reset on the PCIe-attached gateway
        // chip will cascade to its ETH-routed peers.
        tracing::debug!(interface_id, "skipping reset: no kmd device node");
        return Ok(());
    }

    // Stable identity for the chip across the reset cycle.
    let bdf = read_bdf(interface_id)?;

    tracing::info!(interface_id, %bdf, "issuing ASIC_RESET");
    reset_ioctl(&dev_path, RESET_DEVICE_ASIC_RESET)?;

    // ASIC_RESET drops the PCIe link, the device disappears from
    // /sys/bus/pci, and kmd may assign a new /dev/tenstorrent/<N> when it
    // comes back. Re-locate by BDF (the slot is stable).
    let (new_id, new_path) = wait_for_bdf(&bdf, Duration::from_secs(10))?;
    tracing::info!(new_id, %bdf, "device reappeared after ASIC_RESET");

    tracing::info!(new_id, %bdf, "issuing POST_RESET");
    reset_ioctl(&new_path, RESET_DEVICE_POST_RESET)?;
    Ok(())
}

fn read_bdf(interface_id: usize) -> Result<String, Box<dyn std::error::Error>> {
    let device = luwen::kmd::PciDevice::open(interface_id)?;
    let p = &device.physical;
    Ok(format!(
        "{:04x}:{:02x}:{:02x}.{:x}",
        p.pci_domain, p.pci_bus, p.slot, p.pci_function
    ))
}

/// Poll `/dev/tenstorrent/*` until a device with the given BDF reappears,
/// returning its new interface id and `/dev` path.
fn wait_for_bdf(
    target: &str,
    timeout: Duration,
) -> Result<(usize, String), Box<dyn std::error::Error>> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Ok(entries) = std::fs::read_dir("/dev/tenstorrent") {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let Some(id) = name.to_str().and_then(|s| s.parse::<usize>().ok()) else {
                    continue;
                };
                if let Ok(bdf) = read_bdf(id) {
                    if bdf == target {
                        return Ok((id, format!("/dev/tenstorrent/{id}")));
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err(format!("device {target} did not reappear within {timeout:?}").into())
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
