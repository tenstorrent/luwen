// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt::Display;

use luwen_core::Arch;
use thiserror::Error;

use crate::arc_msg::ArcMsgError;

#[derive(Debug)]
pub struct BtWrapper(pub std::backtrace::Backtrace);

impl BtWrapper {
    #[inline(always)]
    pub fn capture() -> Self {
        Self(std::backtrace::Backtrace::capture())
    }
}

impl Display for BtWrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)?;
        Ok(())
    }
}

#[derive(Error, Debug)]
pub enum PlatformError {
    #[error("Tried to initialize chip with the wrong architecture, expected {expected:?} but got {actual:?}\n{backtrace}")]
    WrongChipArch {
        actual: Arch,
        expected: Arch,
        backtrace: BtWrapper,
    },

    #[error("Tried to initialize chip with the wrong architecture, expected one of {expected:?} but got {actual:?}\n{backtrace}")]
    WrongChipArchs {
        actual: Arch,
        expected: Vec<Arch>,
        backtrace: BtWrapper,
    },

    #[error("Unsupported fw version, got {version:x} but required {required:x}")]
    UnsupportedFwVersion { version: u32, required: u32 },

    #[error("It is not currently safe to communicate with ARC because, {0}")]
    ArcNotReady(String),

    #[error(transparent)]
    ArcMsgError(#[from] ArcMsgError),

    #[error("Ethernet training not complete on {} ports", .0.iter().copied().filter(|v| *v).count())]
    EthernetTrainingNotComplete(Vec<bool>),

    #[error(transparent)]
    AxiError(#[from] crate::chip::AxiError),

    #[error("{0}\n{1}")]
    Generic(String, BtWrapper),

    #[error("{0}\n{1}")]
    GenericError(Box<dyn std::error::Error>, BtWrapper),
}

impl From<Box<dyn std::error::Error>> for PlatformError {
    #[inline]
    fn from(e: Box<dyn std::error::Error>) -> Self {
        Self::GenericError(e, BtWrapper::capture())
    }
}

impl From<String> for PlatformError {
    #[inline]
    fn from(e: String) -> Self {
        Self::Generic(e, BtWrapper::capture())
    }
}
