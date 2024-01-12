// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use thiserror::Error;

use kmdif::{PciError, PciOpenError};
use luwen_if::{ArcMsgError, chip::AxiError, error::PlatformError};

#[derive(Error, Debug)]
pub enum LuwenError {
    #[error(transparent)]
    PlatformError(#[from] PlatformError),

    #[error(transparent)]
    PciOpenError(#[from] PciOpenError),

    #[error(transparent)]
    PciError(#[from] PciError),

    #[error("{0}")]
    Custom(String),
}

impl From<ArcMsgError> for LuwenError {
    fn from(value: ArcMsgError) -> Self {
        LuwenError::PlatformError(value.into())
    }
}

impl From<AxiError> for LuwenError {
    fn from(value: AxiError) -> Self {
        LuwenError::PlatformError(value.into())
    }
}
