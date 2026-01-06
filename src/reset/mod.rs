// SPDX-FileCopyrightText: © 2024 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

//! Chip reset functionality.
//!
//! This module provides reset capabilities for Wormhole and Blackhole chips,
//! migrated from tt-tools-common. It supports:
//!
//! - PCIe link reset
//! - Chip-specific reset sequences
//!   - Wormhole: ARC message-based
//!   - Blackhole: `ioctl`-based
//! - State restoration after reset
//! - Batch reset of multiple chips
//!
//! # Examples
//!
//! ```no_run
//! use luwen::reset::{reset_chips, Opts};
//!
//! // Reset all detected chips with default options
//! let result = reset_chips(Opts::default()).unwrap();
//! println!("Reset {} chips successfully", result.successful.len());
//! ```

pub mod blackhole;
pub mod wormhole;

use std::collections::HashSet;
use std::fs::OpenOptions;
use std::os::fd::AsRawFd;
use std::time::{Duration, Instant};

use thiserror::Error;

use crate::api::ChipImpl;
use crate::def::Arch;
use crate::kmd::ioctl::{
    self, ResetDevice, ResetDeviceIn, RESET_DEVICE_RESET_PCIE_LINK, RESET_DEVICE_RESTORE_STATE,
};
use crate::kmd::PciDevice as KmdPciDevice;
use crate::pci::PciDevice;

/// Errors that can occur during chip reset operations.
#[derive(Debug, Error)]
pub enum ResetError {
    /// Failed to open the device file.
    #[error("Failed to open device /dev/tenstorrent/{interface}: {source}")]
    DeviceOpenFailed {
        interface: usize,
        #[source]
        source: std::io::Error,
    },

    /// `ioctl` call failed.
    #[error("ioctl reset failed for interface {interface}: {message}")]
    IoctlFailed { interface: usize, message: String },

    /// Reset did not complete within the timeout.
    #[error("Reset timeout for interface {interface} after {timeout:?}")]
    Timeout { interface: usize, timeout: Duration },

    /// Failed to restore device state.
    #[error("Failed to restore state for interface {interface}: {message}")]
    RestoreFailed { interface: usize, message: String },

    /// Device disappeared after reset.
    #[error("Device {interface} did not reappear after reset")]
    DeviceNotFound { interface: usize },

    /// Failed to open chip for reset.
    #[error("Failed to open chip {interface}: {message}")]
    ChipOpenFailed { interface: usize, message: String },

    /// Platform error during reset.
    #[error("Platform error during reset: {0}")]
    Platform(#[from] crate::api::error::PlatformError),

    /// Generic reset error.
    #[error("{0}")]
    Generic(String),
}

impl ResetError {
    /// Creates a generic reset error with the given message.
    pub fn generic(message: impl Into<String>) -> Self {
        ResetError::Generic(message.into())
    }
}

/// Options for controlling reset behavior.
#[derive(Debug, Clone)]
pub struct Opts {
    /// Timeout for waiting for reset to complete.
    pub timeout: Duration,

    /// Whether to perform PCIe link reset before chip reset.
    pub pcie_link_reset: bool,

    /// Whether to restore device state after reset.
    pub restore_state: bool,

    /// Specific interfaces to reset, or `None` to reset all detected interfaces.
    pub interfaces: Option<Vec<usize>>,
}

impl Default for Opts {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(2),
            pcie_link_reset: true,
            restore_state: true,
            interfaces: None,
        }
    }
}

/// Response from a chip reset operation.
#[derive(Debug)]
pub struct Response {
    /// Interfaces that were successfully reset.
    pub successful: Vec<usize>,

    /// Interfaces that failed to reset, with their errors.
    pub failed: Vec<(usize, ResetError)>,

    /// New interfaces that appeared after reset (unexpected).
    pub new_interfaces: Vec<usize>,

    /// Interfaces that disappeared after reset (problematic).
    pub missing_interfaces: Vec<usize>,
}

impl Response {
    /// Returns `true` if all resets succeeded and no interfaces changed.
    pub fn is_success(&self) -> bool {
        self.failed.is_empty()
            && self.new_interfaces.is_empty()
            && self.missing_interfaces.is_empty()
    }
}

/// Trait for chip-specific reset implementations.
pub trait Reset: Send {
    /// Returns the interface ID for this chip.
    fn interface(&self) -> usize;

    /// Initiates the reset sequence for this chip.
    ///
    /// # Errors
    ///
    /// Returns an error if the reset could not be initiated.
    fn initiate_reset(&mut self) -> Result<(), ResetError>;

    /// Polls whether the reset has completed.
    ///
    /// Returns `true` if reset is complete, `false` if still in progress.
    ///
    /// # Errors
    ///
    /// Returns an error if the reset has failed.
    fn poll_reset_complete(&mut self) -> Result<bool, ResetError>;

    /// Restores device state after reset.
    ///
    /// # Errors
    ///
    /// Returns an error if state restoration fails.
    fn restore_state(&mut self) -> Result<(), ResetError>;
}

/// Performs a PCIe link reset on a device.
///
/// Returns `Ok(true)` if the link was reset successfully, `Ok(false)` if the
/// link was not in a good state but reset was attempted.
///
/// # Errors
///
/// Returns an error if the device cannot be opened or the `ioctl` fails.
pub fn pcie_link_reset(interface: usize) -> Result<bool, ResetError> {
    let fd = OpenOptions::new()
        .read(true)
        .write(true)
        .open(format!("/dev/tenstorrent/{interface}"))
        .map_err(|e| ResetError::DeviceOpenFailed {
            interface,
            source: e,
        })?;

    let mut reset_dev = ResetDevice {
        input: ResetDeviceIn {
            flags: RESET_DEVICE_RESET_PCIE_LINK,
            ..Default::default()
        },
        ..Default::default()
    };

    unsafe { ioctl::reset_device(fd.as_raw_fd(), &mut reset_dev) }.map_err(|e| {
        ResetError::IoctlFailed {
            interface,
            message: format!("{:?}", e),
        }
    })?;

    Ok(reset_dev.output.result == 0)
}

/// Restores device state after reset.
///
/// # Errors
///
/// Returns an error if the device cannot be opened or state restoration fails.
pub(crate) fn restore_device_state(interface: usize) -> Result<(), ResetError> {
    let fd = OpenOptions::new()
        .read(true)
        .write(true)
        .open(format!("/dev/tenstorrent/{interface}"))
        .map_err(|e| ResetError::DeviceOpenFailed {
            interface,
            source: e,
        })?;

    let mut reset_dev = ResetDevice {
        input: ResetDeviceIn {
            flags: RESET_DEVICE_RESTORE_STATE,
            ..Default::default()
        },
        ..Default::default()
    };

    unsafe { ioctl::reset_device(fd.as_raw_fd(), &mut reset_dev) }.map_err(|e| {
        ResetError::IoctlFailed {
            interface,
            message: format!("{:?}", e),
        }
    })?;

    if reset_dev.output.result != 0 {
        return Err(ResetError::RestoreFailed {
            interface,
            message: format!("ioctl returned error code {}", reset_dev.output.result),
        });
    }

    Ok(())
}

/// Creates a reset tracker for a specific chip based on its architecture.
fn create_reset_tracker(interface: usize, arch: Arch) -> Result<Box<dyn Reset>, ResetError> {
    match arch {
        #[allow(deprecated)]
        Arch::Grayskull => Err(ResetError::generic("Grayskull support has been sunset")),
        Arch::Wormhole => Ok(Box::new(wormhole::Reset::new(interface))),
        Arch::Blackhole => Ok(Box::new(blackhole::Reset::new(interface)?)),
    }
}

/// Resets a single chip by interface ID.
///
/// This function performs a full reset sequence:
///
/// 1. PCIe link reset ([_optional_](Opts::pcie_link_reset))
/// 2. Chip-specific reset
/// 3. Wait for reset completion
/// 4. Restore device state ([_optional_](Opts::restore_state))
///
/// # Errors
///
/// Returns an error if the chip cannot be opened, the reset fails, or state
/// restoration fails.
pub fn reset_chip(interface: usize, options: &Opts) -> Result<(), ResetError> {
    // Optionally perform PCIe link reset first
    if options.pcie_link_reset {
        let _ = pcie_link_reset(interface); // Ignore errors, still try chip reset
    }

    // Open device to determine architecture
    let device = KmdPciDevice::open(interface).map_err(|e| ResetError::ChipOpenFailed {
        interface,
        message: format!("{:?}", e),
    })?;

    let mut tracker = create_reset_tracker(interface, device.arch)?;

    // Initiate reset
    tracker.initiate_reset()?;

    // Wait for reset to complete
    let start = Instant::now();
    while start.elapsed() < options.timeout {
        if tracker.poll_reset_complete()? {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    // Restore state if requested
    if options.restore_state {
        tracker.restore_state()?;
    }

    Ok(())
}

/// Resets multiple chips.
///
/// This function provides a high-level interface to reset multiple chips,
/// similar to the tt-tools-common reset functionality.
///
/// # Errors
///
/// This function itself does not return an error, but individual chip failures
/// are recorded in [`Response::failed`].
///
/// # Examples
///
/// ```no_run
/// use luwen::reset::{reset_chips, Opts};
///
/// let result = reset_chips(Opts::default()).unwrap();
/// if result.is_success() {
///     println!("All chips reset successfully!");
/// } else {
///     for (interface, error) in &result.failed {
///         println!("Interface {} failed: {}", interface, error);
///     }
/// }
/// ```
pub fn reset_chips(options: Opts) -> Result<Response, ResetError> {
    // Get initial list of interfaces
    let initial_interfaces: Vec<usize> = match &options.interfaces {
        Some(interfaces) => interfaces.clone(),
        None => PciDevice::scan(),
    };

    let mut trackers: Vec<(Box<dyn Reset>, bool)> = Vec::new();
    let mut failed: Vec<(usize, ResetError)> = Vec::new();

    // Initialize trackers and perform PCIe link reset
    for &interface in &initial_interfaces {
        // Optionally perform PCIe link reset
        if options.pcie_link_reset {
            let _ = pcie_link_reset(interface);
        }

        // Try to open device and create tracker
        match KmdPciDevice::open(interface) {
            Ok(device) => match create_reset_tracker(interface, device.arch) {
                Ok(tracker) => trackers.push((tracker, false)),
                Err(e) => failed.push((interface, e)),
            },
            Err(e) => failed.push((
                interface,
                ResetError::ChipOpenFailed {
                    interface,
                    message: format!("{:?}", e),
                },
            )),
        }
    }

    // Initiate reset on all trackers
    let mut reset_initiated: Vec<usize> = Vec::new();
    for (tracker, _) in trackers.iter_mut() {
        let interface = tracker.interface();
        if let Err(e) = tracker.initiate_reset() {
            failed.push((interface, e));
        } else {
            reset_initiated.push(interface);
        }
    }

    // Remove failed trackers from active list
    trackers.retain(|(tracker, _)| reset_initiated.contains(&tracker.interface()));

    // Wait for all resets to complete
    let start = Instant::now();
    while start.elapsed() < options.timeout {
        let mut all_done = true;
        for (tracker, completed) in trackers.iter_mut() {
            if !*completed {
                match tracker.poll_reset_complete() {
                    Ok(true) => *completed = true,
                    Ok(false) => all_done = false,
                    Err(_) => {
                        // Continue with other trackers on error
                        all_done = false;
                    }
                }
            }
        }

        if all_done {
            break;
        }

        std::thread::sleep(Duration::from_millis(10));
    }

    // Restore state for all trackers
    let mut successful = Vec::new();
    for (mut tracker, _completed) in trackers {
        let interface = tracker.interface();
        if options.restore_state {
            if let Err(e) = tracker.restore_state() {
                failed.push((interface, e));
                continue;
            }
        }
        successful.push(interface);
    }

    // Re-detect chips to verify they're back
    let mut reinit_interfaces: HashSet<usize> = HashSet::new();
    if let Ok(chips) = crate::pci::detect_chips_fallible() {
        for chip in chips {
            if let Ok(chip) = chip.init(&mut |_| Ok::<(), std::convert::Infallible>(())) {
                if let Ok(Some(info)) = chip.get_device_info() {
                    reinit_interfaces.insert(info.interface_id as usize);
                }
            }
        }
    }

    // Check for interface changes
    let initial_set: HashSet<usize> = initial_interfaces.iter().copied().collect();
    let mut new_interfaces: Vec<usize> = reinit_interfaces
        .difference(&initial_set)
        .copied()
        .collect();
    new_interfaces.sort();

    let mut missing_interfaces: Vec<usize> = initial_set
        .difference(&reinit_interfaces)
        .copied()
        .collect();
    missing_interfaces.sort();

    // Mark any missing interfaces as failed
    for interface in &missing_interfaces {
        if !failed.iter().any(|(i, _)| i == interface) {
            failed.push((
                *interface,
                ResetError::DeviceNotFound {
                    interface: *interface,
                },
            ));
        }
        successful.retain(|i| i != interface);
    }

    Ok(Response {
        successful,
        failed,
        new_interfaces,
        missing_interfaces,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_options_default() {
        let opts = Opts::default();
        assert_eq!(opts.timeout, Duration::from_secs(2));
        assert!(opts.pcie_link_reset);
        assert!(opts.restore_state);
        assert!(opts.interfaces.is_none());
    }

    #[test]
    fn test_response_is_success() {
        let result = Response {
            successful: vec![0, 1],
            failed: vec![],
            new_interfaces: vec![],
            missing_interfaces: vec![],
        };
        assert!(result.is_success());

        let result_with_failure = Response {
            successful: vec![0],
            failed: vec![(1, ResetError::generic("test"))],
            new_interfaces: vec![],
            missing_interfaces: vec![],
        };
        assert!(!result_with_failure.is_success());
    }
}
