// SPDX-FileCopyrightText: © 2024 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

//! Wormhole-specific reset implementation.

use crate::api::chip::ArcMsgOptions;
use crate::api::{ArcState, ChipImpl, TypedArcMsg};

use super::{restore_device_state, ResetError};

/// Wormhole-specific reset tracker.
///
/// This implementation uses ARC messages to reset Wormhole chips. The reset
/// sequence sets the ARC state to A3 (safe state) before triggering the reset.
pub struct Reset {
    interface: usize,
}

impl Reset {
    /// Creates a new Wormhole reset tracker for the given interface.
    pub fn new(interface: usize) -> Self {
        Self { interface }
    }
}

impl super::Reset for Reset {
    fn interface(&self) -> usize {
        self.interface
    }

    fn initiate_reset(&mut self) -> Result<(), ResetError> {
        let chip = crate::pci::open(self.interface).map_err(|e| ResetError::ChipOpenFailed {
            interface: self.interface,
            message: format!("{:?}", e),
        })?;

        // First, set ARC state to A3 (safe state for reset)
        chip.arc_msg(ArcMsgOptions {
            msg: TypedArcMsg::SetArcState {
                state: ArcState::A3,
            }
            .into(),
            ..Default::default()
        })
        .map_err(ResetError::Platform)?;

        // Then trigger the reset without waiting for completion
        chip.arc_msg(ArcMsgOptions {
            msg: TypedArcMsg::TriggerReset.into(),
            wait_for_done: false,
            ..Default::default()
        })
        .map_err(ResetError::Platform)?;

        Ok(())
    }

    fn poll_reset_complete(&mut self) -> Result<bool, ResetError> {
        // Wormhole reset completes relatively quickly and doesn't have
        // a reliable polling mechanism, so we always return false and
        // rely on the timeout in the main reset loop.
        Ok(false)
    }

    fn restore_state(&mut self) -> Result<(), ResetError> {
        restore_device_state(self.interface)
    }
}
