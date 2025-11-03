//! Common utilities for luwen tests
//!
//! This module provides shared functionality used across multiple test files
//! in the luwen project. It includes helper functions for detecting hardware,
//! checking chip types, and other test setup operations.

use luwen::api::chip::Chip;

/// Checks if any compatible hardware is available for testing
///
/// Returns false and prints a message if no devices are found or there's an error.
#[allow(dead_code)]
pub fn hardware_available() -> bool {
    match luwen::pci::detect_chips_fallible() {
        Ok(chips) => {
            if chips.is_empty() {
                println!("Test SKIPPED: No devices found");
                return false;
            }
            true
        }
        Err(e) => {
            println!("Test SKIPPED: Error detecting chips: {e}");
            false
        }
    }
}

/// Checks if any chip of a specific type is available
///
/// Takes a predicate function that checks if a chip meets specific criteria.
/// Returns false and prints a message if no matching chips are found.
///
/// # Example
///
/// ```
/// // Check if any Wormhole chips are available
/// if !test_utils::has_chip_type(|chip| chip.as_wh().is_some()) {
///     return; // Skip the test
/// }
/// ```
#[allow(dead_code)]
pub fn has_chip_type<F>(chip_type_check: F) -> bool
where
    F: Fn(&Chip) -> bool,
{
    match luwen::pci::detect_chips_fallible() {
        Ok(chips) => {
            let has_type = chips.iter().any(|chip| {
                if let Some(upgraded) = chip.try_upgrade() {
                    chip_type_check(upgraded)
                } else {
                    false
                }
            });

            if !has_type {
                println!("Test SKIPPED: No matching chip type found");
                return false;
            }
            true
        }
        Err(e) => {
            println!("Test SKIPPED: Error detecting chips: {e}");
            false
        }
    }
}
