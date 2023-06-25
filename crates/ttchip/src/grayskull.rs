use kmdif::PciError;

use crate::{
    common::{ArcMsg, Chip},
    TTError,
};

pub struct Grayskull {
    pub chip: Chip,
}

impl Grayskull {
    pub fn create(device_id: usize) -> Result<Self, TTError> {
        let chip = Chip::create(device_id)?;

        Self::new(chip)
    }

    pub fn new(chip: Chip) -> Result<Self, TTError> {
        if let kmdif::Arch::Grayskull = chip.arch() {
            Ok(Self { chip })
        } else {
            Err(TTError::ArchMismatch {
                expected: kmdif::Arch::Grayskull,
                actual: chip.arch(),
            })
        }
    }

    pub fn arc_msg(
        &mut self,
        msg: &mut ArcMsg,
        wait_for_done: bool,
        timeout: std::time::Duration,
    ) -> Result<crate::common::ArcMsgOk, crate::common::ArcMsgError> {
        self.chip.arc_msg(msg, wait_for_done, timeout, false)
    }
}
