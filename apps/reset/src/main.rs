use std::collections::HashSet;

use luwen::api::ChipImpl;

mod blackhole;
mod wormhole;

trait Reset {
    fn reset(&mut self);
    fn wait(&mut self) -> bool;
    fn restore(&mut self);
}

/// Returns true if the link was okay if false link was not good
fn link_reset(interface: usize) -> bool {
    let fd = if let Ok(fd) = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(format!("/dev/tenstorrent/{interface}"))
    {
        fd
    } else {
        return false;
    };

    let mut reset_device = luwen::kmd::ioctl::ResetDevice {
        input: luwen::kmd::ioctl::ResetDeviceIn {
            flags: luwen::kmd::ioctl::RESET_DEVICE_RESET_PCIE_LINK,
            ..Default::default()
        },
        ..Default::default()
    };
    if unsafe {
        luwen::kmd::ioctl::reset_device(std::os::fd::AsRawFd::as_raw_fd(&fd), &mut reset_device)
    }
    .is_err()
    {
        return false;
    }

    reset_device.output.result == 0
}

fn main() {
    let mut trackers = Vec::new();
    // The interfaces that we expect to be restored post reset...
    let interfaces = luwen::pci::PciDevice::scan();
    println!("Found {} chips to reset", interfaces.len());
    for interface in interfaces.iter().copied() {
        // Just try to reset the link... if it fails then we should still try stuff
        link_reset(interface);
        if let Ok(device) = luwen::kmd::PciDevice::open(interface) {
            let tracker = match device.arch {
                #[expect(deprecated)]
                luwen::def::Arch::Grayskull => unimplemented!("grayskull support has been sunset"),
                luwen::def::Arch::Wormhole => {
                    Box::new(wormhole::ResetTracker::init(interface)) as Box<dyn Reset>
                }
                luwen::def::Arch::Blackhole => {
                    Box::new(blackhole::ResetTracker::init(interface)) as Box<dyn Reset>
                }
            };
            trackers.push((tracker, false));
        }
    }

    println!("Resetting chips");
    for (tracker, _completed) in trackers.iter_mut() {
        tracker.reset();
    }

    println!("Waiting for reset to complete");
    let start = std::time::Instant::now();

    while start.elapsed().as_secs() < 2 {
        let mut all_done = true;
        for (tracker, completed) in trackers.iter_mut() {
            if !*completed {
                all_done = false;
                if tracker.wait() {
                    *completed = true;
                }
            }
        }

        if all_done {
            break;
        }
    }

    let mut failed_reset = Vec::new();
    for (interface, (mut tracker, completed)) in trackers.into_iter().enumerate() {
        if !completed {
            failed_reset.push(interface);
        }
        tracker.restore();
    }

    let mut reinit_interfaces = HashSet::new();

    let chips = luwen::pci::detect_chips_fallible().unwrap();
    for chip in chips {
        let chip = chip
            .init(&mut |_| Ok::<(), std::convert::Infallible>(()))
            .map_err(Into::<luwen::api::error::PlatformError>::into)
            .unwrap();
        if let Ok(Some(info)) = chip.get_device_info() {
            reinit_interfaces.insert(info.interface_id as usize);
        }
    }

    if !failed_reset.is_empty() {
        println!("Failed to reset chips {failed_reset:?}");
    }

    let interfaces = HashSet::from_iter(interfaces);
    let mut new_interfaces = reinit_interfaces
        .difference(&interfaces)
        .copied()
        .collect::<Vec<_>>();
    new_interfaces.sort();
    let mut not_found_interfaces = interfaces
        .difference(&reinit_interfaces)
        .copied()
        .collect::<Vec<_>>();
    not_found_interfaces.sort();

    if !new_interfaces.is_empty() || !not_found_interfaces.is_empty() {
        if !new_interfaces.is_empty() && !not_found_interfaces.is_empty() {
            panic!("We have had interfaces both appearing and disappearing (interfaces that appeared: {new_interfaces:?}; interfaces that vanished: {not_found_interfaces:?})");
        } else if !new_interfaces.is_empty() {
            panic!(
                "Some new interfaces appeared?!? (interfaces that appeared: {new_interfaces:?})"
            );
        } else if !not_found_interfaces.is_empty() {
            panic!(
                "Some interfaces disappeared (interfaces that vanished: {not_found_interfaces:?})"
            );
        }
    }
}
