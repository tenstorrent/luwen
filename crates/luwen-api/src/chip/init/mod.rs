// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use std::convert::Infallible;

use crate::{error::PlatformError, ChipImpl};

use status::InitStatus;

pub mod status;

pub enum CallReason<'a> {
    NewChip,
    NotNew,
    InitWait(&'a InitStatus),
    ChipInitCompleted(&'a InitStatus),
}

#[allow(dead_code)]
pub struct ChipDetectState<'a> {
    pub chip: &'a dyn ChipImpl,
    pub call: CallReason<'a>,
}

#[derive(thiserror::Error)]
pub enum InitError<E> {
    #[error(transparent)]
    PlatformError(#[from] PlatformError),

    CallbackError(E),
}

impl From<InitError<Infallible>> for PlatformError {
    fn from(val: InitError<Infallible>) -> Self {
        match val {
            InitError::PlatformError(err) => err,
            InitError::CallbackError(_) => unreachable!(),
        }
    }
}

/// This function will wait for the chip to be initialized.
/// It will return Ok(true) if the chip initialized successfully.
/// It will return Ok(false) if the chip failed to initialize, but we can continue running.
///     - This is only possible if allow_failure is true.
/// An Err(..) will be returned if the chip failed to initialize and we cannot continue running the chip detection sequence.
///     - In the case that allow_failure is false, Ok(true) will be returned as an error.
///
/// This component makes a callback available which allows the init status to be updated if there
/// is someone/something monitoring the init progress. The initial/driving purpose of this is to
/// track the progress on the command line.
pub fn wait_for_init<E>(
    chip: &mut impl ChipImpl,
    callback: &mut impl FnMut(ChipDetectState) -> Result<(), E>,
    allow_failure: bool,
    noc_safe: bool,
) -> Result<InitStatus, InitError<E>> {
    // We want to make sure that we always call the callback at least once so that the caller can mark the chip presence.
    callback(ChipDetectState {
        chip,
        call: CallReason::NewChip,
    })
    .map_err(|v| InitError::CallbackError(v))?;

    let mut status = InitStatus::new_unknown();
    status.init_options.noc_safe = noc_safe;
    loop {
        match chip.update_init_state(&mut status)? {
            super::ChipInitResult::NoError => {
                // No error, we don't have to do anything.
            }
            super::ChipInitResult::ErrorContinue(error, bt_tracker) => {
                // Hit an error, cannot continue to initialize the current chip,
                // but we can continue to initialize other chips (assuming we are allowing failures).
                if !allow_failure {
                    Err(PlatformError::Generic(
                        format!("Chip initialization failed: {error} \n{status}"),
                        crate::error::BtWrapper(bt_tracker),
                    ))?;
                } else {
                    callback(ChipDetectState {
                        chip,
                        call: CallReason::ChipInitCompleted(&status),
                    })
                    .map_err(InitError::CallbackError)?;
                    return Ok(status);
                }
            }
            super::ChipInitResult::ErrorAbort(error, bt_tracker) => {
                Err(PlatformError::Generic(
                    format!("Chip initialization failed (aborted): {error} \n{status}"),
                    crate::error::BtWrapper(bt_tracker),
                ))?;
            }
        }

        let call = if !status.init_complete() {
            CallReason::InitWait(&status)
        } else {
            // Yes, this also returns a result that we are ignoring.
            // But we are always going to return right after this anyway.
            callback(ChipDetectState {
                chip,
                call: CallReason::ChipInitCompleted(&status),
            })
            .map_err(InitError::CallbackError)?;
            if status.has_error() {
                Err(PlatformError::Generic(
                    format!("Chip initialization failed:\n{status} "),
                    crate::error::BtWrapper::capture(),
                ))?;
            }

            return Ok(status);
        };

        callback(ChipDetectState { chip, call }).map_err(InitError::CallbackError)?;
    }
}
