use std::os::unix::fs::{FileExt, MetadataExt};
use std::path::Path;
use std::time::{Duration, Instant};

/// `TENSTORRENT_RESET_DEVICE_ASIC_RESET` — full ASIC reset (DMC-orchestrated,
/// fired via the `PCIe` interface timer). The device's PCI Command
/// register bit 6 (`PCI_COMMAND_PARITY`) is set as the "in-progress"
/// marker and cleared by the FW once the chip is back to a known-good
/// state.
const RESET_DEVICE_ASIC_RESET: u32 = 4;
/// `TENSTORRENT_RESET_DEVICE_POST_RESET` — issued after the marker has
/// cleared. Restores PCI config and re-inits the kmd state.
const RESET_DEVICE_POST_RESET: u32 = 6;

/// kmd's in-place reset-done marker: bit 6 of byte 4 of PCI config space
/// (the `PCI_COMMAND_PARITY` bit). kmd sets it on `ASIC_RESET`, FW clears
/// it once the chip is initialised.
const PCI_COMMAND_PARITY: u8 = 1 << 6;

pub fn chip_reset(
    interface_id: usize,
    timeout: Duration,
) -> Result<(), Box<dyn std::error::Error>> {
    let dev_path = format!("/dev/tenstorrent/{interface_id}");
    if !Path::new(&dev_path).exists() {
        // Chip detected via Ethernet, not direct PCIe — there's no kmd
        // device node to reset. The reset on the PCIe-attached gateway
        // chip cascades to its ETH-routed peers on the same module.
        tracing::debug!(interface_id, "skipping reset: no kmd device node");
        return Ok(());
    }

    // Stable identity for the chip across the reset cycle. Use the sysfs
    // symlink so we don't mmap a BAR that may go away mid-reset.
    let bdf = bdf_from_sysfs(&dev_path)?;
    let sysfs_dev = format!("/sys/bus/pci/devices/{bdf}");

    tracing::info!(interface_id, %bdf, "issuing ASIC_RESET");
    reset_ioctl(&dev_path, RESET_DEVICE_ASIC_RESET)?;

    // Phase 1: wait for the kernel-side reset-done signal. Mirrors
    // tt-dal/src/dev/reset.c — sysfs-only reads so we don't touch a BAR
    // that's currently unmapped. Handles both the "device disappears and
    // reappears" pathway (hotplug-enabled, e.g. pre-2.8 kmd) and the
    // "device stays on bus, FW clears the marker" pathway (2.8+ Galaxy
    // where `pci_ignore_hotplug` keeps the device bound throughout).
    wait_for_reset_complete(&sysfs_dev, Duration::from_secs(5))?;

    // Phase 2: relocate by BDF. kmd may have shuffled `/dev/tenstorrent/<N>`
    // indices if hotplug processed remove+probe. Read each candidate's
    // BDF from sysfs (no BAR mmap) and pick the match.
    let (new_id, new_path) = relocate_by_bdf(&bdf, timeout)?;
    tracing::info!(new_id, %bdf, "device reappeared after ASIC_RESET");

    tracing::info!(new_id, %bdf, "issuing POST_RESET");
    reset_ioctl(&new_path, RESET_DEVICE_POST_RESET)?;
    Ok(())
}

/// Read a chip's BDF (`0000:01:00.0` style) via sysfs without touching
/// the kmd device. Resolves `/sys/dev/char/<major>:<minor>/device` to its
/// PCI device directory; the basename of the resolved path is the BDF.
fn bdf_from_sysfs(dev_path: &str) -> Result<String, Box<dyn std::error::Error>> {
    let meta = std::fs::metadata(dev_path)?;
    let rdev = meta.rdev();
    // glibc dev_t encoding: major in bits 8..20 and 32..63, minor split
    // across bits 0..8 and 12..32. Most modern Linux installs use this
    // shape regardless of the actual numeric range.
    let major = ((rdev >> 8) & 0xfff) | ((rdev >> 32) & !0xfff);
    let minor = (rdev & 0xff) | ((rdev >> 12) & !0xff);
    let symlink = format!("/sys/dev/char/{major}:{minor}/device");
    let resolved = std::fs::read_link(&symlink)?;
    let bdf = resolved
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("symlink target has no filename")?
        .to_string();
    Ok(bdf)
}

/// Phase 1 of the reset: poll the PCI Command register's bit 6 (set by
/// kmd's `ASIC_RESET` handler, cleared by FW once the chip is back) via
/// sysfs only. Also handles the disappear/reappear cycle on systems where
/// kmd lets hotplug run.
fn wait_for_reset_complete(
    sysfs_dev: &str,
    timeout: Duration,
) -> Result<(), Box<dyn std::error::Error>> {
    let start = Instant::now();
    let mut saw_disappear = false;
    while start.elapsed() < timeout {
        if Path::new(sysfs_dev).exists() {
            if saw_disappear {
                return Ok(());
            }
            // In-place reset path: read PCI Command byte 4 and watch for
            // the `PCI_COMMAND_PARITY` bit to clear.
            if let Ok(file) = std::fs::OpenOptions::new()
                .read(true)
                .open(format!("{sysfs_dev}/config"))
            {
                let mut buf = [0u8; 1];
                if file.read_exact_at(&mut buf, 4).is_ok() && buf[0] & PCI_COMMAND_PARITY == 0 {
                    return Ok(());
                }
            }
        } else {
            saw_disappear = true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err(format!("reset did not complete within {timeout:?}").into())
}

/// Phase 2: find which `/dev/tenstorrent/<N>` now corresponds to the
/// chip we just reset. kmd may shuffle indices when hotplug remove+probe
/// runs; on 2.8+ Galaxy (hotplug suppressed + stable Galaxy ordinals)
/// the index doesn't change at all and we match on the first probe.
fn relocate_by_bdf(
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
                let candidate_path = format!("/dev/tenstorrent/{id}");
                if let Ok(bdf) = bdf_from_sysfs(&candidate_path) {
                    if bdf == target {
                        return Ok((id, candidate_path));
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_millis(200));
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
            // kmd writes `result` into the output region up to this many
            // bytes; on versions that honour it, a zero would leave the
            // status undefined.
            output_size_bytes: u32::try_from(
                std::mem::size_of::<luwen::kmd::ioctl::ResetDeviceOut>(),
            )
            .expect("ResetDeviceOut fits in u32"),
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
