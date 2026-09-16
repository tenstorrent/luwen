// SPDX-FileCopyrightText: © 2026 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

//! Flash-oriented init helpers.
//!
//! tt-flash only needs the ARC mailbox (and PCIe comms) to talk SPI. GDDR
//! train/BIST failures must not block that path.

/// `error_status0` bit for `INIT_STAGE_GDDR_TRAIN` (CMFW `init_stage_id`).
pub const INIT_STAGE_GDDR_TRAIN: u32 = 1 << 4;

/// Simplified ARC firmware boot view used to decide flash readiness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FwBoot {
    NotStarted,
    Started,
    Done,
    Error,
    Unknown,
    Unreadable,
}

/// Result of asking "can we flash despite a borked GDDR/FW boot?"
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlashArcOutcome {
    /// Still waiting for the ARC mailbox to come up.
    Waiting(Option<String>),
    /// Mailbox is up; flash can proceed. `warning` is set when FW is unhappy.
    Ready { warning: Option<String> },
    /// Cannot read FW status and mailbox is not safe; do not flash.
    Fatal(String),
}

/// Decide whether ARC is usable for SPI flash.
///
/// Flash is allowed as soon as the mailbox is safe. A FW boot `Error`
/// (typically GDDR train/BIST) becomes a warning, not a hard failure.
pub fn evaluate_flash_arc(
    fw: FwBoot,
    msg_safe: bool,
    error_status0: Option<u32>,
) -> FlashArcOutcome {
    if msg_safe {
        return FlashArcOutcome::Ready {
            warning: flash_warning(fw, error_status0),
        };
    }

    match fw {
        FwBoot::Unreadable => FlashArcOutcome::Fatal(
            "Failed to access fw to read init status; ARC mailbox is not safe".to_string(),
        ),
        FwBoot::Error => FlashArcOutcome::Waiting(Some(
            "BH FW boot error; waiting for ARC mailbox before flash".to_string(),
        )),
        FwBoot::Started | FwBoot::NotStarted | FwBoot::Unknown | FwBoot::Done => {
            FlashArcOutcome::Waiting(Some(
                "Waiting for ARC mailbox (flash-safe; GDDR status ignored)".to_string(),
            ))
        }
    }
}

fn flash_warning(fw: FwBoot, error_status0: Option<u32>) -> Option<String> {
    match fw {
        FwBoot::Error => Some(gddr_warning(error_status0)),
        FwBoot::Started => {
            Some("BH FW boot not complete; ARC mailbox is up so flash can proceed".to_string())
        }
        FwBoot::Unknown => {
            Some("BH FW boot status unknown; ARC mailbox is up so flash can proceed".to_string())
        }
        FwBoot::NotStarted | FwBoot::Done | FwBoot::Unreadable => None,
    }
}

fn gddr_warning(error_status0: Option<u32>) -> String {
    match error_status0 {
        Some(v) if v & INIT_STAGE_GDDR_TRAIN != 0 => format!(
            "BH FW boot error includes GDDR train/BIST (error_status0=0x{v:x}); flashing anyway"
        ),
        Some(v) => format!("BH FW boot error (error_status0=0x{v:x}); flashing anyway"),
        None => "BH FW boot error; flashing anyway".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gddr_train_error_is_ready_when_mailbox_safe() {
        let outcome = evaluate_flash_arc(FwBoot::Error, true, Some(INIT_STAGE_GDDR_TRAIN));
        match outcome {
            FlashArcOutcome::Ready { warning } => {
                let warning = warning.expect("GDDR train error should warn");
                assert!(warning.contains("GDDR train/BIST"), "{warning}");
                assert!(warning.contains("0x10"), "{warning}");
            }
            other => panic!("expected Ready, got {other:?}"),
        }
    }

    #[test]
    fn gddr_train_error_keeps_waiting_without_mailbox() {
        let outcome = evaluate_flash_arc(FwBoot::Error, false, Some(INIT_STAGE_GDDR_TRAIN));
        assert!(matches!(outcome, FlashArcOutcome::Waiting(_)));
    }

    #[test]
    fn done_and_safe_is_ready_without_warning() {
        let outcome = evaluate_flash_arc(FwBoot::Done, true, None);
        assert_eq!(outcome, FlashArcOutcome::Ready { warning: None });
    }

    #[test]
    fn unreadable_without_mailbox_is_fatal() {
        let outcome = evaluate_flash_arc(FwBoot::Unreadable, false, None);
        assert!(matches!(outcome, FlashArcOutcome::Fatal(_)));
    }
}
