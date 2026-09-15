// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use std::convert::Infallible;

use crate::{error::PlatformError, ChipImpl};

use status::{InitOptions, InitStatus};

pub mod flash;
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
    wait_for_init_with_options(
        chip,
        callback,
        allow_failure,
        InitOptions {
            noc_safe,
            flash_safe: false,
        },
    )
}

/// Same as [`wait_for_init`], but accepts full [`InitOptions`].
///
/// `flash_safe` lets init succeed when the chip can still talk SPI (PCIe + ARC
/// mailbox) even if GDDR train/BIST left ARC firmware in an error state.
pub fn wait_for_init_with_options<E>(
    chip: &mut impl ChipImpl,
    callback: &mut impl FnMut(ChipDetectState) -> Result<(), E>,
    allow_failure: bool,
    options: InitOptions,
) -> Result<InitStatus, InitError<E>> {
    // We want to make sure that we always call the callback at least once so that the caller can mark the chip presence.
    callback(ChipDetectState {
        chip,
        call: CallReason::NewChip,
    })
    .map_err(|v| InitError::CallbackError(v))?;

    let mut status = InitStatus::new_unknown();
    status.init_options = options;
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
            if status.has_error() && !flash_safe_init_ok(&status) {
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

/// Flash-safe init accepts a chip that can communicate and whose ARC is no
/// longer waiting, even if DRAM/ETH/CPU reported errors.
pub(crate) fn flash_safe_init_ok(status: &InitStatus) -> bool {
    status.init_options.flash_safe
        && status.can_communicate()
        && !status.arc_status.is_waiting()
        && !status.arc_status.has_error()
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use luwen_def::Arch;

    use super::*;
    use crate::{
        chip::{
            communication::{
                chip_comms::{AxiData, AxiError, ChipComms},
                chip_interface::ChipInterface,
            },
            hl_comms::HlComms,
            init::{
                flash::{evaluate_flash_arc, FlashArcOutcome, FwBoot, INIT_STAGE_GDDR_TRAIN},
                status::{ArcInitError, CommsStatus, ComponentStatusInfo, WaitStatus},
            },
            ArcMsgOptions, ChipInitResult, NeighbouringChip, Telemetry,
        },
        error::{ArcReadyError, PlatformError},
        ArcMsgOk, DeviceInfo, EthAddr,
    };

    struct DummyComms;
    impl ChipComms for DummyComms {
        fn axi_translate(&self, _addr: &str) -> Result<AxiData, AxiError> {
            Err(AxiError::NoAxiData)
        }
        fn axi_read(
            &self,
            _chip_if: &dyn ChipInterface,
            _addr: u64,
            _data: &mut [u8],
        ) -> Result<(), Box<dyn std::error::Error>> {
            Err("unused".into())
        }
        fn axi_write(
            &self,
            _chip_if: &dyn ChipInterface,
            _addr: u64,
            _data: &[u8],
        ) -> Result<(), Box<dyn std::error::Error>> {
            Err("unused".into())
        }
        fn noc_read(
            &self,
            _chip_if: &dyn ChipInterface,
            _noc_id: u8,
            _x: u8,
            _y: u8,
            _addr: u64,
            _data: &mut [u8],
        ) -> Result<(), Box<dyn std::error::Error>> {
            Err("unused".into())
        }
        fn noc_write(
            &self,
            _chip_if: &dyn ChipInterface,
            _noc_id: u8,
            _x: u8,
            _y: u8,
            _addr: u64,
            _data: &[u8],
        ) -> Result<(), Box<dyn std::error::Error>> {
            Err("unused".into())
        }
        fn noc_multicast(
            &self,
            _chip_if: &dyn ChipInterface,
            _noc_id: u8,
            _start: (u8, u8),
            _end: (u8, u8),
            _addr: u64,
            _data: &[u8],
        ) -> Result<(), Box<dyn std::error::Error>> {
            Err("unused".into())
        }
        fn noc_broadcast(
            &self,
            _chip_if: &dyn ChipInterface,
            _noc_id: u8,
            _addr: u64,
            _data: &[u8],
        ) -> Result<(), Box<dyn std::error::Error>> {
            Err("unused".into())
        }
    }

    struct DummyIf;
    impl ChipInterface for DummyIf {
        fn get_device_info(&self) -> Result<Option<DeviceInfo>, Box<dyn std::error::Error>> {
            Ok(None)
        }
        fn axi_read(
            &self,
            _addr: u32,
            _data: &mut [u8],
        ) -> Result<(), Box<dyn std::error::Error>> {
            Err("unused".into())
        }
        fn axi_write(&self, _addr: u32, _data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
            Err("unused".into())
        }
        fn noc_read(
            &self,
            _noc_id: u8,
            _x: u8,
            _y: u8,
            _addr: u64,
            _data: &mut [u8],
        ) -> Result<(), Box<dyn std::error::Error>> {
            Err("unused".into())
        }
        fn noc_write(
            &self,
            _noc_id: u8,
            _x: u8,
            _y: u8,
            _addr: u64,
            _data: &[u8],
        ) -> Result<(), Box<dyn std::error::Error>> {
            Err("unused".into())
        }
        fn noc_broadcast(
            &self,
            _noc_id: u8,
            _addr: u64,
            _data: &[u8],
        ) -> Result<(), Box<dyn std::error::Error>> {
            Err("unused".into())
        }
        fn noc_multicast(
            &self,
            _noc_id: u8,
            _start: (u8, u8),
            _end: (u8, u8),
            _addr: u64,
            _data: &[u8],
        ) -> Result<(), Box<dyn std::error::Error>> {
            Err("unused".into())
        }
        fn eth_noc_read(
            &self,
            _eth_addr: EthAddr,
            _noc_id: u8,
            _x: u8,
            _y: u8,
            _addr: u64,
            _data: &mut [u8],
        ) -> Result<(), Box<dyn std::error::Error>> {
            Err("unused".into())
        }
        fn eth_noc_write(
            &self,
            _eth_addr: EthAddr,
            _noc_id: u8,
            _x: u8,
            _y: u8,
            _addr: u64,
            _data: &[u8],
        ) -> Result<(), Box<dyn std::error::Error>> {
            Err("unused".into())
        }
        fn eth_noc_multicast(
            &self,
            _eth_addr: EthAddr,
            _noc_id: u8,
            _start: (u8, u8),
            _end: (u8, u8),
            _addr: u64,
            _data: &[u8],
        ) -> Result<(), Box<dyn std::error::Error>> {
            Err("unused".into())
        }
        fn eth_noc_broadcast(
            &self,
            _eth_addr: EthAddr,
            _noc_id: u8,
            _addr: u64,
            _data: &[u8],
        ) -> Result<(), Box<dyn std::error::Error>> {
            Err("unused".into())
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    /// Chip whose FW reports a GDDR train/BIST error but the ARC mailbox is up.
    struct GddrFailedChip {
        comms: DummyComms,
        iface: DummyIf,
        msg_safe: bool,
    }

    impl GddrFailedChip {
        fn new(msg_safe: bool) -> Self {
            Self {
                comms: DummyComms,
                iface: DummyIf,
                msg_safe,
            }
        }
    }

    impl HlComms for GddrFailedChip {
        fn comms_obj(&self) -> (&dyn ChipComms, &dyn ChipInterface) {
            (&self.comms, &self.iface)
        }
    }

    impl ChipImpl for GddrFailedChip {
        fn update_init_state(
            &mut self,
            status: &mut InitStatus,
        ) -> Result<ChipInitResult, PlatformError> {
            if status.unknown_state {
                let opts = std::mem::take(&mut status.init_options);
                *status = InitStatus {
                    comms_status: CommsStatus::CanCommunicate,
                    arc_status: ComponentStatusInfo {
                        name: "ARC".to_string(),
                        wait_status: Box::new([WaitStatus::Waiting(None)]),
                        start_time: std::time::Instant::now(),
                        timeout: std::time::Duration::from_secs(5),
                    },
                    dram_status: ComponentStatusInfo::init_waiting(
                        "DRAM".to_string(),
                        std::time::Duration::from_secs(1),
                        8,
                    ),
                    eth_status: ComponentStatusInfo::init_waiting(
                        "ETH".to_string(),
                        std::time::Duration::from_secs(1),
                        1,
                    ),
                    cpu_status: ComponentStatusInfo::init_waiting(
                        "CPU".to_string(),
                        std::time::Duration::from_secs(1),
                        1,
                    ),
                    init_options: opts,
                    warnings: Vec::new(),
                    unknown_state: false,
                };
            }

            status.comms_status = CommsStatus::CanCommunicate;
            for s in status.dram_status.wait_status.iter_mut() {
                *s = WaitStatus::Done;
            }
            for s in status.eth_status.wait_status.iter_mut() {
                *s = WaitStatus::Done;
            }
            for s in status.cpu_status.wait_status.iter_mut() {
                *s = WaitStatus::Done;
            }

            let flash_safe = status.init_options.flash_safe;
            let mut warnings = Vec::new();
            for s in status.arc_status.wait_status.iter_mut() {
                match s {
                    WaitStatus::Waiting(_) | WaitStatus::JustFinished => {
                        if flash_safe {
                            match evaluate_flash_arc(
                                FwBoot::Error,
                                self.msg_safe,
                                Some(INIT_STAGE_GDDR_TRAIN),
                            ) {
                                FlashArcOutcome::Ready { warning } => {
                                    if let Some(warning) = warning {
                                        warnings.push(warning);
                                    }
                                    *s = WaitStatus::Done;
                                }
                                FlashArcOutcome::Waiting(_) => {}
                                FlashArcOutcome::Fatal(_) => {
                                    *s = WaitStatus::Error(ArcInitError::WaitingForInit(
                                        ArcReadyError::NoAccess,
                                    ));
                                }
                            }
                        } else {
                            *s = WaitStatus::Error(ArcInitError::WaitingForInit(
                                ArcReadyError::BootError,
                            ));
                        }
                    }
                    _ => {}
                }
            }
            status.warnings.extend(warnings);
            Ok(ChipInitResult::NoError)
        }

        fn get_arch(&self) -> Arch {
            Arch::Blackhole
        }

        fn get_telemetry(&self) -> Result<Telemetry, PlatformError> {
            Err(PlatformError::Generic(
                "unused".to_string(),
                crate::error::BtWrapper::capture(),
            ))
        }

        fn arc_msg(&self, _msg: ArcMsgOptions) -> Result<ArcMsgOk, PlatformError> {
            Err(PlatformError::Generic(
                "unused".to_string(),
                crate::error::BtWrapper::capture(),
            ))
        }

        fn get_neighbouring_chips(&self) -> Result<Vec<NeighbouringChip>, PlatformError> {
            Ok(vec![])
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }

        fn get_device_info(&self) -> Result<Option<DeviceInfo>, PlatformError> {
            Ok(None)
        }
    }

    #[test]
    fn wait_for_init_fails_on_gddr_error_by_default() {
        let mut chip = GddrFailedChip::new(true);
        let result = wait_for_init::<Infallible>(&mut chip, &mut |_| Ok(()), false, false);
        assert!(result.is_err(), "strict init must fail when GDDR/FW is borked");
    }

    #[test]
    fn wait_for_init_flash_safe_ignores_gddr_error() {
        let mut chip = GddrFailedChip::new(true);
        let status = match wait_for_init_with_options::<Infallible>(
            &mut chip,
            &mut |_| Ok(()),
            true,
            InitOptions {
                noc_safe: true,
                flash_safe: true,
            },
        ) {
            Ok(status) => status,
            Err(_) => panic!("flash-safe init should return the chip despite GDDR train failure"),
        };

        assert!(status.can_communicate());
        assert!(!status.arc_status.has_error());
        assert!(
            status
                .warnings
                .iter()
                .any(|w| w.contains("GDDR train/BIST")),
            "expected GDDR warning, got {:?}",
            status.warnings
        );
        assert!(flash_safe_init_ok(&status));
    }
}
