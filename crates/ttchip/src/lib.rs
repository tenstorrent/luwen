use thiserror::Error;

mod common;
mod grayskull;
mod wormhole;

use kmdif::{Arch, PciError, PciOpenError};
use common::Chip;

pub use kmdif::DmaConfig;
pub use grayskull::Grayskull;
pub use wormhole::Wormhole;
pub use common::ArcMsg;

#[derive(Error, Debug)]
pub enum TTError {
    #[error(transparent)]
    PciOpenError(#[from] PciOpenError),

    #[error(transparent)]
    PciError(#[from] PciError),

    #[error("Chip arch mismatch: expected {expected:?}, got {actual:?}")]
    ArchMismatch { expected: Arch, actual: Arch },

    #[error("Unkown chip arch {0}")]
    UnknownArch(u16),
}

pub enum AllChips {
    Wormhole(Wormhole),
    Grayskull(Grayskull),
}

pub fn create_chip(device_id: usize) -> Result<AllChips, TTError> {
    let chip = Chip::create(device_id)?;

    match chip.arch() {
        Arch::Grayskull => Ok(AllChips::Grayskull(Grayskull::new(chip)?)),
        Arch::Wormhole => Ok(AllChips::Wormhole(Wormhole::new(chip)?)),
        Arch::Unknown(v) => Err(TTError::UnknownArch(v)),
    }
}
