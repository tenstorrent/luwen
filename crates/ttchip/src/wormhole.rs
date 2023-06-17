use crate::{
    common::{ArcMsg, Chip},
    TTError,
};

pub struct Wormhole {
    pub chip: Chip,
}

impl Wormhole {
    pub fn create(device_id: usize) -> Result<Self, TTError> {
        let chip = Chip::create(device_id)?;

        Self::new(chip)
    }

    pub fn new(chip: Chip) -> Result<Self, TTError> {
        if let kmdif::Arch::Wormhole = chip.arch() {
            Ok(Self { chip })
        } else {
            Err(TTError::ArchMismatch {
                expected: kmdif::Arch::Wormhole,
                actual: chip.arch(),
            })
        }
    }

    pub fn arc_msg(
        &mut self,
        msg: &mut ArcMsg,
        wait_for_done: bool,
        timeout: std::time::Duration,
        use_second_mailbox: bool,
    ) -> Result<crate::common::ArcMsgOk, crate::common::ArcMsgError> {
        self.chip
            .arc_msg(msg, wait_for_done, timeout, use_second_mailbox)
    }
}
