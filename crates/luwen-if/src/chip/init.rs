use crate::{ChipImpl, error::PlatformError};

use super::StatusInfo;

pub enum CallReason<'a> {
    NewChip,
    InitWait(&'a str, &'a StatusInfo),
    ChipInitCompleted
}

#[allow(dead_code)]
pub struct ChipDetectState<'a> {
    pub chip: &'a dyn ChipImpl,
    pub call: CallReason<'a>,
}

pub fn wait_for_init(chip: &impl ChipImpl, callback: &mut impl FnMut(ChipDetectState<'_>)) -> Result<(), PlatformError> {
    // We want to make sure that we always call the callback at least once so that the caller can mark the chip presence.
    callback(ChipDetectState {
        chip,
        call: CallReason::NewChip,
    });

    let mut status = chip.is_inititalized()?;
    loop {
        chip.update_init_state(&mut status)?;

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
            callback(ChipDetectState {
                chip,
                call: CallReason::ChipInitCompleted,
            });
            return Ok(())
        }

        callback(state);
    }
}
