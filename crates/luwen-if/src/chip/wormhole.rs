// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{backtrace, sync::Arc};

use crate::{
    arc_msg::{ArcMsgAddr, ArcMsgOk, TypedArcMsg},
    chip::{
        communication::{
            chip_comms::{load_axi_table, ChipComms},
            chip_interface::ChipInterface,
        },
        hl_comms::HlCommsInterface,
    },
    error::{BtWrapper, PlatformError},
    ArcMsg, ChipImpl, IntoChip,
};

use super::{
    eth_addr::EthAddr,
    hl_comms::HlComms,
    init::status::{ComponentStatusInfo, EthernetPartialInitError, InitOptions, WaitStatus},
    remote::{EthAddresses, RemoteArcIf},
    ArcMsgOptions, ChipInitResult, CommsStatus, InitStatus, NeighbouringChip,
};

/// Implementation of the interface for a Wormhole
/// both the local and remote Wormhole chips are represented by this struct
#[derive(Clone)]
pub struct Wormhole {
    pub chip_if: Arc<dyn ChipInterface + Send + Sync>,
    pub arc_if: Arc<dyn ChipComms + Send + Sync>,

    pub is_remote: bool,
    pub use_arc_for_spi: bool,

    pub arc_addrs: ArcMsgAddr,
    pub eth_locations: [EthCore; 16],
    pub eth_addrs: EthAddresses,
    telemetry_addr: Arc<once_cell::sync::OnceCell<u32>>,
}

impl HlComms for Wormhole {
    fn comms_obj(&self) -> (&dyn ChipComms, &dyn ChipInterface) {
        (self.arc_if.as_ref(), self.chip_if.as_ref())
    }
}

impl HlComms for &Wormhole {
    fn comms_obj(&self) -> (&dyn ChipComms, &dyn ChipInterface) {
        (self.arc_if.as_ref(), self.chip_if.as_ref())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct EthCore {
    pub x: u8,
    pub y: u8,
    pub enabled: bool,
}

impl Default for EthCore {
    fn default() -> Self {
        Self {
            x: 0,
            y: 0,
            enabled: true,
        }
    }
}

impl Wormhole {
    pub(crate) fn init<
        CC: ChipComms + Send + Sync + 'static,
        CI: ChipInterface + Send + Sync + 'static,
    >(
        is_remote: bool,
        use_arc_for_spi: bool,
        arc_if: CC,
        chip_if: CI,
    ) -> Result<Self, PlatformError> {
        // let mut version = [0; 4];
        // arc_if.axi_read(&chip_if, 0x0, &mut version);
        // let version = u32::from_le_bytes(version);
        let _version = 0x0;

        let output = Wormhole {
            chip_if: Arc::new(chip_if),

            is_remote,
            use_arc_for_spi,

            arc_addrs: ArcMsgAddr {
                scratch_base: arc_if.axi_translate("ARC_RESET.SCRATCH[0]")?.addr,
                arc_misc_cntl: arc_if.axi_translate("ARC_RESET.ARC_MISC_CNTL")?.addr,
            },

            arc_if: Arc::new(arc_if),
            eth_addrs: EthAddresses::default(),

            telemetry_addr: Arc::new(once_cell::sync::OnceCell::new()),

            eth_locations: [
                EthCore {
                    x: 9,
                    y: 0,
                    ..Default::default()
                },
                EthCore {
                    x: 1,
                    y: 0,
                    ..Default::default()
                },
                EthCore {
                    x: 8,
                    y: 0,
                    ..Default::default()
                },
                EthCore {
                    x: 2,
                    y: 0,
                    ..Default::default()
                },
                EthCore {
                    x: 7,
                    y: 0,
                    ..Default::default()
                },
                EthCore {
                    x: 3,
                    y: 0,
                    ..Default::default()
                },
                EthCore {
                    x: 6,
                    y: 0,
                    ..Default::default()
                },
                EthCore {
                    x: 4,
                    y: 0,
                    ..Default::default()
                },
                EthCore {
                    x: 9,
                    y: 6,
                    ..Default::default()
                },
                EthCore {
                    x: 1,
                    y: 6,
                    ..Default::default()
                },
                EthCore {
                    x: 8,
                    y: 6,
                    ..Default::default()
                },
                EthCore {
                    x: 2,
                    y: 6,
                    ..Default::default()
                },
                EthCore {
                    x: 7,
                    y: 6,
                    ..Default::default()
                },
                EthCore {
                    x: 3,
                    y: 6,
                    ..Default::default()
                },
                EthCore {
                    x: 6,
                    y: 6,
                    ..Default::default()
                },
                EthCore {
                    x: 4,
                    y: 6,
                    ..Default::default()
                },
            ],
        };

        Ok(output)
    }

    pub fn init_eth_addrs(&mut self) -> Result<(), PlatformError> {
        if self.eth_addrs.masked_version == 0 {
            let telemetry = self.get_telemetry()?;

            self.eth_addrs = EthAddresses::new(telemetry.eth_fw_version);
        }

        Ok(())
    }

    pub fn get_if<T: ChipInterface>(&self) -> Option<&T> {
        self.chip_if.as_any().downcast_ref::<T>()
    }

    pub fn open_remote(&self, addr: impl IntoChip<EthAddr>) -> Result<Wormhole, PlatformError> {
        let arc_if = RemoteArcIf {
            addr: addr.cinto(&self.arc_if, &self.chip_if).unwrap(),
            axi_data: Some(load_axi_table("wormhole-axi-noc.bin", 0)),
        };

        Self::init(true, true, arc_if, self.chip_if.clone())
    }

    // fn check_dram_trained(&self) {
    //     let pc = self.axi_sread32("ARC_RESET.POST_CODE")?;

    //     0x29
    // }

    fn check_arc_msg_safe(&self, msg_reg: u64, _return_reg: u64) -> Result<(), PlatformError> {
        const POST_CODE_INIT_DONE: u32 = 0xC0DE0001;
        const _POST_CODE_ARC_MSG_HANDLE_START: u32 = 0xC0DE0030;
        const POST_CODE_ARC_MSG_HANDLE_DONE: u32 = 0xC0DE003F;
        const POST_CODE_ARC_TIME_LAST: u32 = 0xC0DE007F;

        let s5 = self.axi_sread32(format!("ARC_RESET.SCRATCH[{msg_reg}]"))?;
        let pc = self.axi_sread32("ARC_RESET.POST_CODE")?;
        let dma = self.axi_sread32("ARC_CSM.ARC_PCIE_DMA_REQUEST.trigger")?;

        if pc == 0xFFFFFFFF {
            return Err(PlatformError::ArcNotReady(
                crate::error::ArcReadyError::NoAccess,
                BtWrapper::capture(),
            ))?;
        }

        if s5 == 0xDEADC0DE {
            return Err(PlatformError::ArcNotReady(
                crate::error::ArcReadyError::WatchdogTriggered,
                BtWrapper::capture(),
            ))?;
        }

        // Still booting and it will later wipe SCRATCH[5/2].
        if s5 == 0x00000060 || pc == 0x11110000 {
            return Err(PlatformError::ArcNotReady(
                crate::error::ArcReadyError::BootIncomplete,
                BtWrapper::capture(),
            ))?;
        }

        if s5 == 0x0000AA00 || s5 == TypedArcMsg::ArcGoToSleep.msg_code() as u32 {
            return Err(PlatformError::ArcNotReady(
                crate::error::ArcReadyError::Asleep,
                BtWrapper::capture(),
            ))?;
        }

        // PCIE DMA writes SCRATCH[5] on exit, so it's not safe.
        // Also we assume FW is hung if we see this state.
        // (The former is only relevant when msg_reg==5, but the latter is always relevant.)
        if dma != 0 {
            return Err(PlatformError::ArcNotReady(
                crate::error::ArcReadyError::OutstandingPcieDMA,
                BtWrapper::capture(),
            ))?;
        }

        if s5 & 0xFFFFFF00 == 0x0000AA00 {
            let message_id = s5 & 0xFF;
            return Err(PlatformError::ArcNotReady(
                crate::error::ArcReadyError::MessageQueued(message_id),
                BtWrapper::capture(),
            ))?;
        }

        if s5 & 0xFF00FFFF == 0xAA000000 {
            let message_id = (s5 >> 16) & 0xFF;
            return Err(PlatformError::ArcNotReady(
                crate::error::ArcReadyError::HandlingMessage(message_id),
                BtWrapper::capture(),
            ))?;
        }

        // Boot complete (new FW only), message not recognized,
        // pcie_dma_{chip_to_host,host_to_chip}_transfer failed to acquire PCIE mutex
        if let 0x00000001 | 0xFFFFFFFF | 0xFFFFDEAD = s5 {
            return Ok(());
        }

        if s5 & 0x0000FFFF > 0x00000001 {
            // YYYY00XX for XX != 0, 1
            // Message complete, response written into s5. Post code might not be set back to idle yet, but it will happen.
            return Ok(());
        }

        if s5 == 0 {
            // not yet booted or L2 init or old FW finished boot, or old FW processing message
            // or pcie_dma_{chip_to_host,host_to_chip}_transfer completed

            // Some of these also represent short-term busy, but it's safe to write SCRATCH[5].
            let pc_idle = pc == POST_CODE_INIT_DONE
                || (POST_CODE_ARC_MSG_HANDLE_DONE..=POST_CODE_ARC_TIME_LAST).contains(&pc);
            if pc_idle {
                return Ok(());
            } else {
                return Err(PlatformError::ArcNotReady(
                    crate::error::ArcReadyError::OldPostCode(pc),
                    BtWrapper::capture(),
                ))?;
            }
        }

        // We should never get here, every case should be handled above.
        Ok(())
    }

    pub fn spi_write(&self, addr: u32, value: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        let spi = super::spi::ActiveSpi::new(self, self.use_arc_for_spi)?;

        spi.write(self, addr, value)?;

        Ok(())
    }

    pub fn spi_read(&self, addr: u32, value: &mut [u8]) -> Result<(), Box<dyn std::error::Error>> {
        let spi = super::spi::ActiveSpi::new(self, self.use_arc_for_spi)?;

        spi.read(self, addr, value)?;

        Ok(())
    }
}

fn default_status() -> InitStatus {
    InitStatus {
        comms_status: super::CommsStatus::CanCommunicate,
        arc_status: ComponentStatusInfo {
            name: "ARC".to_string(),
            wait_status: Box::new([WaitStatus::Waiting(None)]),

            start_time: std::time::Instant::now(),
            timeout: std::time::Duration::from_secs(300),
        },
        dram_status: ComponentStatusInfo::init_waiting(
            "DRAM".to_string(),
            std::time::Duration::from_secs(300),
            4,
        ),
        eth_status: ComponentStatusInfo::init_waiting(
            "ETH".to_string(),
            std::time::Duration::from_secs(15 * 60),
            16,
        ),
        cpu_status: ComponentStatusInfo::not_present("CPU".to_string()),

        init_options: InitOptions { noc_safe: false },

        unknown_state: false,
    }
}

impl ChipImpl for Wormhole {
    fn update_init_state(
        &mut self,
        status: &mut InitStatus,
    ) -> Result<ChipInitResult, PlatformError> {
        if status.unknown_state {
            let init_options = std::mem::take(&mut status.init_options);
            *status = default_status();
            status.init_options = init_options;
        }

        let comms = &mut status.comms_status;

        {
            let status = &mut status.arc_status;
            for arc_status in status.wait_status.iter_mut() {
                match arc_status {
                    WaitStatus::Waiting(status_string) => {
                        match self.check_arc_msg_safe(5, 3) {
                            Ok(_) => *arc_status = WaitStatus::JustFinished,
                            Err(err) => {
                                match err {
                                    PlatformError::ArcNotReady(reason, _) => {
                                        // There are three possibilities when trying to get a response
                                        // here. 1. 0xffffffff in this case we want to assume this is some
                                        // sort of AxiError and abort the init. 2. An error we may
                                        // eventually recover from, i.e. arc booting... 3. we have hit an
                                        // error that won't resolve but isn't indicative of further
                                        // problems. For example watchdog triggered.
                                        match reason {
                                            // This is triggered when the s5 or pc registers readback
                                            // 0xffffffff. I am treating it like an AXI error and will
                                            // assume something has gone terribly wrong and abort.
                                            crate::error::ArcReadyError::NoAccess
                                            | crate::error::ArcReadyError::BootError
                                            | crate::error::ArcReadyError::WatchdogTriggered
                                            | crate::error::ArcReadyError::Asleep
                                            | crate::error::ArcReadyError::OldPostCode(_) => {
                                                *arc_status = WaitStatus::Error(super::init::status::ArcInitError::WaitingForInit(reason));
                                            }
                                            crate::error::ArcReadyError::BootIncomplete
                                            | crate::error::ArcReadyError::OutstandingPcieDMA
                                            | crate::error::ArcReadyError::MessageQueued(_)
                                            | crate::error::ArcReadyError::HandlingMessage(_) => {
                                                *status_string = Some(reason.to_string());
                                                if status.start_time.elapsed() > status.timeout {
                                                    *arc_status = WaitStatus::Error(super::init::status::ArcInitError::WaitingForInit(reason));
                                                }
                                            }
                                        }
                                    }

                                    PlatformError::UnsupportedFwVersion { version, required } => {
                                        *arc_status = WaitStatus::Error(
                                            super::init::status::ArcInitError::FwVersionTooOld {
                                                version,
                                                required,
                                            },
                                        );
                                    }

                                    // The fact that this is here means that our result is too generic, for now we just ignore it.
                                    PlatformError::ArcMsgError(error) => {
                                        return Ok(ChipInitResult::ErrorContinue(
                                            error.to_string(),
                                            backtrace::Backtrace::capture(),
                                        ));
                                    }

                                    PlatformError::MessageError(error) => {
                                        return Ok(ChipInitResult::ErrorContinue(
                                            error.to_string(),
                                            backtrace::Backtrace::capture(),
                                        ));
                                    }

                                    // This is fine to hit at this stage (though it should have been already verified to not be the case).
                                    // For now we just ignore it and hope that it will be resolved by the time the timeout expires...
                                    PlatformError::EthernetTrainingNotComplete(_) => {
                                        if let WaitStatus::Waiting(status_string) = arc_status {
                                            if status.start_time.elapsed() > status.timeout {
                                                *arc_status = WaitStatus::Timeout(status.timeout);
                                            } else {
                                                *status_string = Some("Waiting on arc/ethernet; this is unexpected but we'll assume that things will clear up if we wait.".to_string());
                                            }
                                        }
                                    }

                                    // This is an "expected error" but we probably can't recover from it, so we should abort the init.
                                    PlatformError::AxiError(error) => {
                                        *comms = CommsStatus::CommunicationError(error.to_string());
                                        return Ok(ChipInitResult::ErrorAbort(
                                            format!("ARC AXI error: {error}"),
                                            backtrace::Backtrace::capture(),
                                        ));
                                    }

                                    // We don't expect to hit these cases so if we do, we should assume that something went terribly
                                    // wrong and abort the init.
                                    PlatformError::WrongChipArch {
                                        actual,
                                        expected,
                                        backtrace,
                                    } => {
                                        return Ok(ChipInitResult::ErrorAbort(
                                            format!(
                                                "expected chip: {expected}, actual detected chip: {actual}"
                                            ),
                                            backtrace.0,
                                        ))
                                    }

                                    PlatformError::WrongChipArchs {
                                        actual,
                                        expected,
                                        backtrace,
                                    } => {
                                        let expected_chips = expected
                                            .iter()
                                            .map(|arch| arch.to_string())
                                            .collect::<Vec<_>>()
                                            .join(", ");
                                        return Ok(ChipInitResult::ErrorAbort(
                                            format!(
                                                "expected chip: {expected_chips}, actual detected chips: {actual}"
                                            ),
                                            backtrace.0,
                                        ));
                                    }

                                    PlatformError::Generic(error, backtrace) => {
                                        return Ok(ChipInitResult::ErrorAbort(error, backtrace.0));
                                    }

                                    PlatformError::GenericError(error, backtrace) => {
                                        let err_msg = error.to_string();
                                        return Ok(ChipInitResult::ErrorAbort(
                                            err_msg,
                                            backtrace.0,
                                        ));
                                    }
                                }
                            }
                        }
                    }
                    WaitStatus::JustFinished => {
                        *arc_status = WaitStatus::Done;
                    }
                    _ => {}
                }
            }
        }

        // If ARC has not finished initialization then we shouldn't init eth or dram.
        if !status.arc_status.is_waiting() {
            // If something went wrong with ARC then we probably don't have DRAM
            if !status.arc_status.has_error() {
                let arc_status = &mut status.arc_status;
                let status = &mut status.dram_status;

                // We don't need to get the telemetry if we aren't waiting to see if dram has
                // trained...
                if status.is_waiting() {
                    // Hitting an error here implies that the something changed and we no longer have arc
                    // access. We will abort, but allow other chips to continue or not using the typical
                    // switch block.
                    let telem = match self.get_telemetry() {
                        Ok(telem) => Some(telem),
                        Err(err) => match err {
                            // Something did go wrong here, but we'll assume that this is a result of
                            // some temporary issue
                            PlatformError::ArcNotReady(_, _)
                            // Not really expected but ethernet training may not yet be complete so
                            // we'll just ignore it for now.
                            | PlatformError::EthernetTrainingNotComplete(_) => {
                                for dram_status in status.wait_status.iter_mut() {
                                    if let WaitStatus::Waiting(status_string) = dram_status {
                                        if status.start_time.elapsed() > status.timeout {
                                            *dram_status = WaitStatus::Timeout(status.timeout);
                                        } else {
                                            *status_string = Some("Waiting on arc/ethernet; this is unexpected but we'll assume that things will clear up if we wait.".to_string());
                                        }
                                    }
                                }

                                None
                            }


                            PlatformError::UnsupportedFwVersion { version, required } => {
                                if let Some(status) = arc_status.wait_status.get_mut(0) {
                                    *status = WaitStatus::Error(crate::chip::init::status::ArcInitError::FwVersionTooOld {
                                        version,
                                        required,
                                    });
                                }
                                None
                            }

                            // Arc should be ready here...
                            // This means that ARC hung, we should stop initializing this chip in case
                            // we hit something like a noc hang.
                            PlatformError::ArcMsgError(error) => {
                                return Ok(ChipInitResult::ErrorContinue(format!("Telemetry ARC message error: {error}; we expected to have communication, but lost it."), backtrace::Backtrace::capture()));
                            }

                            PlatformError::MessageError(error) => {
                                return Ok(ChipInitResult::ErrorContinue(format!("Telemetry ARC message error: {error:?}; we expected to have communication, but lost it."), backtrace::Backtrace::capture()));
                            }

                            // This is an "expected error" but we probably can't recover from it, so we should abort the init.
                            PlatformError::AxiError(error) => return Ok(ChipInitResult::ErrorAbort(error.to_string(), backtrace::Backtrace::capture())),

                            // We don't expect to hit these cases so if we do, we should assume that something went terribly
                            // wrong and abort the init.
                            PlatformError::WrongChipArch {actual, expected, backtrace} => {
                                return Ok(ChipInitResult::ErrorAbort(format!("expected chip: {expected}, actual detected chip: {actual}"), backtrace.0))
                            }

                            PlatformError::WrongChipArchs {actual, expected, backtrace} => {
                                let expected_chips = expected.iter().map(|arch| arch.to_string()).collect::<Vec<_>>().join(", ");
                                return Ok(ChipInitResult::ErrorAbort(format!("expected chip: {expected_chips}, actual detected chips: {actual}"), backtrace.0));
                            }

                            PlatformError::Generic(error, backtrace) => {
                                return Ok(ChipInitResult::ErrorAbort(error, backtrace.0));
                            }

                            | PlatformError::GenericError(error, backtrace) => {
                                let err_msg = error.to_string();
                                return Ok(ChipInitResult::ErrorAbort(err_msg, backtrace.0));
                            }
                        },
                    };

                    if let Some(telem) = telem {
                        let dram_status = telem.ddr_status;

                        let mut channels = [None; 6];
                        for (i, channel) in channels.iter_mut().enumerate() {
                            let status = (dram_status >> (i * 4)) & 0xF;
                            let status = status as u8;

                            *channel =
                                super::init::status::DramChannelStatus::try_from(status).ok();
                        }

                        for (dram_status, channel_status) in
                            status.wait_status.iter_mut().zip(channels)
                        {
                            match dram_status {
                                WaitStatus::Waiting(status_string) => {
                                    if let Some(
                                        super::init::status::DramChannelStatus::TrainingPass,
                                    ) = channel_status
                                    {
                                        *dram_status = WaitStatus::Done;
                                    } else {
                                        *status_string = Some(
                                            channel_status
                                                .map(|v| v.to_string())
                                                .unwrap_or("Unknown".to_string()),
                                        );
                                        if status.start_time.elapsed() > status.timeout {
                                            if let Some(status) = channel_status {
                                                *dram_status = WaitStatus::Error(
                                                    super::init::status::DramInitError::NotTrained(
                                                        status,
                                                    ),
                                                );
                                            } else {
                                                *dram_status = WaitStatus::Timeout(status.timeout);
                                            }
                                        }
                                    }
                                }
                                WaitStatus::JustFinished => {
                                    *dram_status = WaitStatus::Done;
                                }
                                _ => {}
                            }
                        }
                    }
                }
            } else {
                for dram_status in status.dram_status.wait_status.iter_mut() {
                    *dram_status = WaitStatus::NoCheck;
                }
            }
        } else {
            for dram_status in status.dram_status.wait_status.iter_mut() {
                if let WaitStatus::Waiting(status_string) = dram_status {
                    *status_string = Some("Waiting for ARC".to_string());
                }
            }
        }

        // If ARC has not finished initialization then we shouldn't init eth.
        if !status.arc_status.is_waiting() {
            // We need arc to be alive so that we can check which cores are enabled
            if !status.arc_status.has_error() {
                // Only do eth training if board type is not UBB
                // By this point arc should be alive so we can safely access telem
                let telem = self.get_telemetry()?;
                let board_type: u64 =
                    telem.board_id_low as u64 | ((telem.board_id_high as u64) << 32);
                let board_upi: u64 = (board_type >> 36) & 0xFFFFF;
                const WH_6U_GLX_UPI: u64 = 0x35;

                if board_upi != WH_6U_GLX_UPI {
                    // Only try to initiliaze the ethernet if we are not in noc_safe mode.
                    if !status.init_options.noc_safe {
                        let status = &mut status.eth_status;

                        // We don't need to get the eth training status if we aren't waiting to see if dram has
                        // trained...
                        if status.is_waiting() {
                            let eth_training_status = match self.check_ethernet_training_complete() {
                                Ok(eth_status) => eth_status,
                                Err(err) => match err {
                                    // ARC should be initialized at this point, hitting an error here means
                                    // that we can no longer progress in the init.
                                    PlatformError::ArcMsgError(error) => {
                                        return Ok(ChipInitResult::ErrorContinue(
                                            error.to_string(),
                                            backtrace::Backtrace::capture(),
                                        ));
                                    }

                                    PlatformError::MessageError(error) => {
                                        return Ok(ChipInitResult::ErrorContinue(
                                            error.to_string(),
                                            backtrace::Backtrace::capture(),
                                        ));
                                    }

                                    PlatformError::ArcNotReady(error, backtrace) => {
                                        return Ok(ChipInitResult::ErrorContinue(
                                            error.to_string(),
                                            backtrace.0,
                                        ));
                                    }

                                    // We are checking for ethernet training to complete... if we hit this than
                                    // something has gone terribly wrong
                                    PlatformError::EthernetTrainingNotComplete(eth_cores) => {
                                        let false_count = eth_cores.iter().filter(|&&x| !x).count();
                                        return Ok(ChipInitResult::ErrorContinue(
                                            format!(
                                                "Ethernet training not complete on [{false_count}/16] ports"
                                            ),
                                            backtrace::Backtrace::capture(),
                                        ));
                                    }

                                    // This is an "expected error" but we probably can't recover from it, so we should abort the init.
                                    PlatformError::AxiError(error) => {
                                        return Ok(ChipInitResult::ErrorAbort(
                                            error.to_string(),
                                            backtrace::Backtrace::capture(),
                                        ));
                                    }

                                    // We don't expect to hit these cases so if we do, we should assume that something went terribly
                                    // wrong and abort the init.
                                    PlatformError::UnsupportedFwVersion { version, required } => {
                                        return Ok(ChipInitResult::ErrorAbort(format!("Required Ethernet Firmware Version: {required}, current version: {version:?}"), backtrace::Backtrace::capture()));
                                    }
                                    PlatformError::WrongChipArch {
                                        actual,
                                        expected,
                                        backtrace,
                                    } => {
                                        return Ok(ChipInitResult::ErrorAbort(
                                            format!(
                                            "expected chip: {expected}, actual detected chip: {actual}"
                                        ),
                                            backtrace.0,
                                        ))
                                    }

                                    PlatformError::WrongChipArchs {
                                        actual,
                                        expected,
                                        backtrace,
                                    } => {
                                        let expected_chips = expected
                                            .iter()
                                            .map(|arch| arch.to_string())
                                            .collect::<Vec<_>>()
                                            .join(", ");
                                        return Ok(ChipInitResult::ErrorAbort(
                                            format!(
                                                "expected chip: {expected_chips}, actual detected chips: {actual}"
                                            ),
                                            backtrace.0,
                                        ));
                                    }

                                    PlatformError::Generic(error, backtrace) => {
                                        return Ok(ChipInitResult::ErrorAbort(error, backtrace.0));
                                    }

                                    PlatformError::GenericError(error, backtrace) => {
                                        let err_msg = error.to_string();
                                        return Ok(ChipInitResult::ErrorAbort(err_msg, backtrace.0));
                                    }
                                },
                            };
                            for (eth_status, training_complete) in
                                status.wait_status.iter_mut().zip(eth_training_status)
                            {
                                match eth_status {
                                    WaitStatus::Waiting(status_string) => {
                                        if training_complete {
                                            if let Err(_err) = self.check_ethernet_fw_version() {
                                                *eth_status = WaitStatus::NotInitialized(
                                                    EthernetPartialInitError::FwOverwritten,
                                                );
                                            } else {
                                                *eth_status = WaitStatus::JustFinished;
                                            }
                                        } else if status.start_time.elapsed() > status.timeout {
                                            *eth_status = WaitStatus::Timeout(status.timeout);
                                        } else {
                                            *status_string = Some(format!(
                                                "{}: Waiting for initial training to complete",
                                                self.get_local_chip_coord()?
                                            ));
                                        }
                                    }
                                    WaitStatus::JustFinished => {
                                        *eth_status = WaitStatus::Done;
                                    }
                                    _ => {}
                                }
                            }
                        }
                    } else {
                        let status = &mut status.eth_status;
                        for eth_status in status.wait_status.iter_mut() {
                            *eth_status = WaitStatus::Done;
                        }
                    }
                } else {
                    // If WH UBB - skip ethernet training check
                    let status = &mut status.eth_status;
                    for eth_status in status.wait_status.iter_mut() {
                        *eth_status = WaitStatus::Done;
                    }
                }
            } else {
                let status = &mut status.eth_status;
                for eth_status in status.wait_status.iter_mut() {
                    *eth_status = WaitStatus::NoCheck;
                }
            }
        } else {
            for eth_status in status.eth_status.wait_status.iter_mut() {
                if let WaitStatus::Waiting(status_string) = eth_status {
                    *status_string = Some("Waiting for ARC".to_string());
                }
            }
        }

        {
            // This is not present in wormhole.
            let _status = &mut status.cpu_status;
        }

        Ok(ChipInitResult::NoError)
    }

    fn get_arch(&self) -> luwen_core::Arch {
        luwen_core::Arch::Wormhole
    }

    fn arc_msg(&self, msg: ArcMsgOptions) -> Result<ArcMsgOk, PlatformError> {
        let (msg_reg, return_reg) = if msg.use_second_mailbox {
            (2, 4)
        } else {
            (5, 3)
        };

        self.check_arc_msg_safe(msg_reg, return_reg)?;

        crate::arc_msg::arc_msg(
            self,
            &msg.msg,
            msg.wait_for_done,
            msg.timeout,
            msg_reg,
            return_reg,
            msg.addrs.as_ref().unwrap_or(&self.arc_addrs),
        )
    }

    fn get_neighbouring_chips(&self) -> Result<Vec<NeighbouringChip>, crate::error::PlatformError> {
        const ETH_UNKNOWN: u32 = 0;
        const ETH_UNCONNECTED: u32 = 1;
        const ETH_NO_ROUTING: u32 = 2;

        const SHELF_OFFSET: u64 = 9;
        const RACK_OFFSET: u64 = 10;

        let mut output = Vec::with_capacity(self.eth_locations.len());

        for (
            eth_id,
            EthCore {
                x: eth_x, y: eth_y, ..
            },
        ) in self.eth_locations.iter().copied().enumerate()
        {
            let port_status = self.arc_if.noc_read32(
                &self.chip_if,
                0,
                eth_x,
                eth_y,
                self.eth_addrs.eth_conn_info + (eth_id as u64 * 4),
            )?;

            if port_status == ETH_UNCONNECTED || port_status == ETH_UNKNOWN {
                continue;
            }

            // HACK(drosen): It's not currently possible to route galaxy->nb...
            // This is a limitation of the current ethernet firmware routing scheme,
            // but fixing it would require a large-ish firmware update and lots of testing so
            // for now we are just ignoreing those routes.

            // Get the neighbour's board type
            let next_board_type = self.noc_read32(
                0,
                eth_x,
                eth_y,
                0x1ec0 + (self.eth_addrs.erisc_remote_board_type_offset * 4),
            )?;

            // Get the our board type
            let our_board_type = self.noc_read32(
                0,
                eth_x,
                eth_y,
                0x1ec0 + (self.eth_addrs.erisc_local_board_type_offset * 4),
            )?;

            // Check if it's possble to have routing disabled
            let erisc_routing_disabled =
                self.noc_read32(0, eth_x, eth_y, self.eth_addrs.boot_params + (19 * 4))? == 1;

            // The board type value will be 0 if galaxy and non-zero if nb
            // It's currently not possible to go from GALAXY->NB
            let routing_disabled = (our_board_type == 0 && next_board_type != 0)
                || (erisc_routing_disabled && port_status == ETH_NO_ROUTING);

            // Decode the remote eth_addr for our erisc core
            // This can be used to build a map of the full mesh
            let remote_id = self.noc_read32(
                0,
                eth_x,
                eth_y,
                self.eth_addrs.node_info + (4 * RACK_OFFSET),
            )?;
            let remote_rack_x = remote_id & 0xFF;
            let remote_rack_y = (remote_id >> 8) & 0xFF;

            let remote_id = self.noc_read32(
                0,
                eth_x,
                eth_y,
                self.eth_addrs.node_info + (4 * SHELF_OFFSET),
            )?;
            let remote_shelf_x = (remote_id >> 16) & 0x3F;
            let remote_shelf_y = (remote_id >> 22) & 0x3F;

            let remote_noc_x = (remote_id >> 4) & 0x3F;
            let remote_noc_y = (remote_id >> 10) & 0x3F;

            output.push(NeighbouringChip {
                routing_enabled: !routing_disabled,
                local_noc_addr: (eth_x, eth_y),
                remote_noc_addr: (remote_noc_x as u8, remote_noc_y as u8),
                eth_addr: EthAddr {
                    shelf_x: remote_shelf_x as u8,
                    shelf_y: remote_shelf_y as u8,
                    rack_x: remote_rack_x as u8,
                    rack_y: remote_rack_y as u8,
                },
            });
        }

        Ok(output)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn get_telemetry(&self) -> Result<super::Telemetry, PlatformError> {
        let offset: Result<u32, PlatformError> = self
            .telemetry_addr
            .get_or_try_init(|| {
                let result = self.arc_msg(ArcMsgOptions {
                    msg: ArcMsg::Typed(TypedArcMsg::GetSmbusTelemetryAddr),
                    ..Default::default()
                })?;

                let offset = match result {
                    ArcMsgOk::Ok { arg, .. } => arg,
                    ArcMsgOk::OkNoWait => todo!(),
                };

                Ok(offset)
            })
            .copied();

        let offset = offset?;

        let csm_offset = self.arc_if.axi_translate("ARC_CSM.DATA[0]")?;

        let telemetry_struct_offset = csm_offset.addr + (offset - 0x10000000) as u64;
        let enum_version = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset)?;
        let device_id = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + 4)?;
        let asic_ro = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (2 * 4))?;
        let asic_idd = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (3 * 4))?;

        let board_id_high = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (4 * 4))?;
        let board_id_low = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (5 * 4))?;
        let arc0_fw_version = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (6 * 4))?;
        let arc1_fw_version = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (7 * 4))?;
        let arc2_fw_version = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (8 * 4))?;
        let arc3_fw_version = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (9 * 4))?;
        let spibootrom_fw_version = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (10 * 4))?;
        let eth_fw_version = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (11 * 4))?;
        let m3_bl_fw_version = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (12 * 4))?;
        let m3_app_fw_version = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (13 * 4))?;
        let ddr_status = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (14 * 4))?;
        let eth_status0 = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (15 * 4))?;
        let eth_status1 = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (16 * 4))?;
        let pcie_status = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (17 * 4))?;
        let faults = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (18 * 4))?;
        let arc0_health = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (19 * 4))?;
        let arc1_health = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (20 * 4))?;
        let arc2_health = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (21 * 4))?;
        let arc3_health = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (22 * 4))?;
        let fan_speed = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (23 * 4))?;
        let aiclk = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (24 * 4))?;
        let axiclk = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (25 * 4))?;
        let arcclk = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (26 * 4))?;
        let throttler = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (27 * 4))?;
        let vcore = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (28 * 4))?;
        let asic_temperature = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (29 * 4))?;
        let vreg_temperature = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (30 * 4))?;
        let board_temperature = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (31 * 4))?;
        let tdp = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (32 * 4))?;
        let tdc = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (33 * 4))?;
        let vdd_limits = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (34 * 4))?;
        let thm_limits = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (35 * 4))?;
        let wh_fw_date = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (36 * 4))?;
        let asic_tmon0 = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (37 * 4))?;
        let asic_tmon1 = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (38 * 4))?;
        let mvddq_power = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (39 * 4))?;
        let gddr_train_temp0 = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (40 * 4))?;
        let gddr_train_temp1 = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (41 * 4))?;
        let boot_date = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (42 * 4))?;
        let rt_seconds = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (43 * 4))?;
        let eth_debug_status0 = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (44 * 4))?;
        let eth_debug_status1 = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (45 * 4))?;
        let tt_flash_version = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (46 * 4))?;

        let threshold: u32 = 0x02190000; // arc fw 2.25.0.0
        let fw_bundle_version: u32 = if arc0_fw_version >= threshold {
            self.arc_if
                .axi_read32(&self.chip_if, telemetry_struct_offset + (49 * 4))?
        } else {
            0
        };

        Ok(super::Telemetry {
            arch: self.get_arch(),
            board_id: ((board_id_high as u64) << 32) | (board_id_low as u64),
            enum_version,
            device_id,
            asic_ro,
            asic_idd,
            board_id_high,
            board_id_low,
            arc0_fw_version,
            arc1_fw_version,
            arc2_fw_version,
            arc3_fw_version,
            spibootrom_fw_version,
            eth_fw_version,
            m3_bl_fw_version,
            m3_app_fw_version,
            ddr_status,
            eth_status0,
            eth_status1,
            pcie_status,
            faults,
            arc0_health,
            arc1_health,
            arc2_health,
            arc3_health,
            fan_speed,
            aiclk,
            axiclk,
            arcclk,
            throttler,
            vcore,
            asic_temperature,
            vreg_temperature,
            board_temperature,
            tdp,
            tdc,
            vdd_limits,
            thm_limits,
            wh_fw_date,
            asic_tmon0,
            asic_tmon1,
            mvddq_power,
            gddr_train_temp0,
            gddr_train_temp1,
            boot_date,
            rt_seconds,
            eth_debug_status0,
            eth_debug_status1,
            tt_flash_version,
            fw_bundle_version,
            timer_heartbeat: arc0_health,
            ..Default::default()
        })
    }

    fn get_device_info(&self) -> Result<Option<crate::DeviceInfo>, PlatformError> {
        if self.is_remote {
            Ok(None)
        } else {
            Ok(self.chip_if.get_device_info()?)
        }
    }
}
