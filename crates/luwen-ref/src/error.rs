// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use luwen_if::{chip::AxiError, error::PlatformError, ArcMsgError};
use thiserror::Error;
use ttkmd_if::{PciError, PciOpenError};

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
