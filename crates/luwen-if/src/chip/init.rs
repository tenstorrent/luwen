// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{ChipImpl, error::PlatformError};

use super::{InitStatus, StatusInfo};

pub enum CallReason<'a> {
    NewChip,
    InitWait(&'a str, &'a StatusInfo),
    ChipInitCompleted(InitStatus),
}

#[allow(dead_code)]
pub struct ChipDetectState<'a> {
    pub chip: &'a dyn ChipImpl,
    pub call: CallReason<'a>,
}

#[allow(dead_code)]
pub enum EthernetInitState {
    NotPresent,
    FwCorrupted,
    NotTrained,
    Ready,
}

#[allow(dead_code)]
pub enum ArcInitState {
    FwCorrupted,
    WaitingForInit,
    Hung,
    Ready,
}

#[allow(dead_code)]
pub struct ChipInitState {
    pub can_access: bool,
    pub ethernet_state: EthernetInitState,
    pub arc_state: ArcInitState,

    underlying_chip: super::Chip,
}

#[allow(dead_code)]
impl ChipInitState {
    pub fn get_chip(self) -> super::Chip {
        self.underlying_chip
    }
}

pub fn wait_for_init(
    chip: &impl ChipImpl,
    callback: &mut impl FnMut(ChipDetectState<'_>),
    allow_failure: bool,
) -> Result<bool, PlatformError> {
    // We want to make sure that we always call the callback at least once so that the caller can mark the chip presence.
    callback(ChipDetectState {
        chip,
        call: CallReason::NewChip,
    });

    let mut status = chip.is_inititalized()?;
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
                        call: CallReason::ChipInitCompleted(status),
                    });
                    return Ok(false);
                }
            }
            super::ChipInitResult::ErrorAbort => {
                return Err(PlatformError::Generic(
                    "Chip initialization failed".to_string(),
                    crate::error::BtWrapper::capture(),
                ));
            }
        }

        let mut state = ChipDetectState {
            chip,
            call: CallReason::NewChip,
        };

        if !status.arc_status.is_completed() {
            state.call = CallReason::InitWait("ARC", &status.arc_status);
        } else if !status.dram_status.is_completed() {
            state.call = CallReason::InitWait("DRAM", &status.dram_status);
        } else if !status.eth_status.is_completed() {
            state.call = CallReason::InitWait("ETH", &status.eth_status);
        } else if !status.cpu_status.is_completed() {
            state.call = CallReason::InitWait("CPU", &status.cpu_status);
        } else {
            // Yes, this also returns a result that we are ignoring.
            // But we are always going to return right after this anyway.
            callback(ChipDetectState {
                chip,
                call: CallReason::ChipInitCompleted(status),
            });
            return Ok(true);
        }

        callback(state)
    }
}
