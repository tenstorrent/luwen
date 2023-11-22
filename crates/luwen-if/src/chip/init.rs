// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{error::PlatformError, ChipImpl};
use super::{InitStatus, StatusInfo};
use std::{fmt::{self, Display}, sync::Arc};

pub enum CallReason<'a, E: std::fmt::Display, M: std::fmt::Display> {
    NewChip,
    InitWait(&'a str, &'a StatusInfo<E, M>),
    ChipInitCompleted(&'a InitStatus),
}

#[allow(dead_code)]
pub struct ChipDetectState<'a, E: std::fmt::Display, M: std::fmt::Display> {
    pub chip: &'a dyn ChipImpl,
    pub call: CallReason<'a, E, M>,
}

#[derive(Clone, Debug)]
pub enum EthernetInitError {
    FwCorrupted,
    NotTrained,
}

impl fmt::Display for EthernetInitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EthernetInitError::FwCorrupted => f.write_str("Ethernet firmware is corrupted"),
            EthernetInitError::NotTrained => f.write_str("Ethernet is not trained"),
        }
    }
}

#[derive(Clone, Debug)]
pub enum EthernetPartiallyInitError {
    FwOverwritten,
}

impl fmt::Display for EthernetPartiallyInitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EthernetPartiallyInitError::FwOverwritten => {
                f.write_str("Ethernet firmware version is overwritten")
            }
        }
    }
}

#[derive(Clone, Debug)]
pub enum ArcInitError {
    FwCorrupted,
    WaitingForInit,
    Hung,
}

impl fmt::Display for ArcInitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArcInitError::FwCorrupted => f.write_str("ARC firmware is corrupted"),
            ArcInitError::WaitingForInit => f.write_str("ARC is waiting for initialization"),
            ArcInitError::Hung => f.write_str("ARC is hung"),
        }
    }
    
}

#[derive(Clone, Debug)]
pub enum ArcPartiallyInitError {
    //TODO: Add more errors here.
}

impl fmt::Display for ArcPartiallyInitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            _ => f.write_str("ARC is completely initialized"),
        }
    }
}

#[derive(Clone, Debug)]
pub enum DramInitError {
    // TODO: Add more errors here.
}

impl fmt::Display for DramInitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            _ => f.write_str("DRAM is completely initialized"),
        }
    }
}

#[derive(Clone, Debug)]
pub enum DramPartiallyInitError {
    //TODO: Add more errors here.
}

impl fmt::Display for DramPartiallyInitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            _ => f.write_str("DRAM is completely initialized"),
        }
    }
}

#[derive(Clone, Debug)]
pub enum CpuInitError {
    //TODO: Add more errors here.
}

impl fmt::Display for CpuInitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            _ => f.write_str("CPU is completely initialized"),
        }
    }
}

#[derive(Clone, Debug)]
pub enum CpuPartiallyInitError {
    //TODO: Add more errors here.
}

impl fmt::Display for CpuPartiallyInitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            _ => f.write_str("CPU is completely initialized"),
        }
    }
}

/// This function will wait for the chip to be initialized.
/// It will return Ok(true) if the chip initialized successfully.
/// It will return Ok(false) if the chip failed to initialize, but we can continue running.
///     - This is only possible if allow_failure is true.
/// An Err(..) will be returned if the chip failed to initialize and we cannot continue running the chip detection sequence.
///     - In the case that allow_failure is false, Ok(true) will be returned as an error.
pub fn wait_for_init(
    chip: &impl ChipImpl,
    callback: &mut impl FnMut(ChipDetectState<'_, &dyn std::fmt::Display, &dyn std::fmt::Display>),
    allow_failure: bool,
    noc_safe: bool,
) -> Result<InitStatus, PlatformError> {
    // We want to make sure that we always call the callback at least once so that the caller can mark the chip presence.
    callback(ChipDetectState {
        chip,
        call: CallReason::NewChip,
    });

    let mut status = chip.is_inititalized()?;
    status.init_options.noc_safe = noc_safe;
    loop {
        match chip.update_init_state(&mut status)? {
            super::ChipInitResult::NoError => {
                // No error, we don't have to do anything.
            }
            super::ChipInitResult::ErrorContinue => {
                // Hit an error, cannot continue to initialize the current chip,
                // but we can continue to initialize other chips (assuming we are allowing failures).
                if !allow_failure {
                    return Err(PlatformError::Generic(
                        "Chip initialization failed".to_string(),
                        crate::error::BtWrapper::capture(),
                    ));
                } else {
                    callback(ChipDetectState {
                        chip,
                        call: CallReason::ChipInitCompleted(&status),
                    });
                    return Ok(status);
                }
            }
            super::ChipInitResult::ErrorAbort => {
                return Err(PlatformError::Generic(
                    "Chip initialization failed".to_string(),
                    crate::error::BtWrapper::capture(),
                ));
            }
        }

        let mut state: ChipDetectState<'_, &dyn Display, &dyn Display> = ChipDetectState {
            chip,
            call: CallReason::NewChip,
        };

        if !status.arc_status.init_partially() {
            state.call = CallReason::InitWait("ARC", &status.arc_status.make_dyn());
            
        } else if !status.dram_status.init_partially() {
            state.call = CallReason::InitWait("DRAM", &status.dram_status.make_dyn());
        } else if !status.eth_status.init_partially() {

            state.call = CallReason::InitWait("ETH", &status.eth_status.make_dyn());
        } else if !status.cpu_status.init_partially() {
            state.call = CallReason::InitWait("CPU", &status.cpu_status.make_dyn());
        } else {
            // Yes, this also returns a result that we are ignoring.
            // But we are always going to return right after this anyway.
            println!("Chip initialization complete");
            callback(ChipDetectState {
                chip,
                call: CallReason::ChipInitCompleted(&status),
            });
            return Ok(status);
        }

        // callback(state)
    }
}
