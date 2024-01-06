// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use luwen_core::Arch;

use crate::{
    arc_msg::{ArcMsgAddr, ArcMsgOk, ArcMsgProtocolError},
    chip::HlCommsInterface,
    error::{BtWrapper, PlatformError},
    ArcMsg, ChipImpl,
};

use super::{
    init::status::{ComponentStatusInfo, InitOptions, WaitStatus},
    ArcMsgOptions, ChipComms, ChipInitResult, ChipInterface, HlComms, InitStatus, NeighbouringChip, CommsStatus,
};

#[derive(Clone)]
pub struct Grayskull {
    pub chip_if: Arc<dyn ChipInterface + Send + Sync>,
    pub arc_if: Arc<dyn ChipComms + Send + Sync>,

    pub arc_addrs: ArcMsgAddr,
}

impl Grayskull {
    pub fn get_if<T: ChipInterface>(&self) -> Option<&T> {
        self.chip_if.as_any().downcast_ref::<T>()
    }

    fn check_arc_msg_safe(&self, msg_reg: u64, _return_reg: u64) -> Result<(), PlatformError> {
        const POST_CODE_INIT_DONE: u32 = 0xC0DE0001;
        const _POST_CODE_ARC_MSG_HANDLE_START: u32 = 0xC0DE0030;

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

        if s5 == 0x0000AA00 || s5 == ArcMsg::ArcGoToSleep.msg_code() as u32 {
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

            if pc == POST_CODE_INIT_DONE {
                return Ok(());
            } else {
                return Err(PlatformError::ArcNotReady(
                    crate::error::ArcReadyError::PostCodeBusy(pc),
                    BtWrapper::capture(),
                ))?;
            }
        }

        // We should never get here, every case should be handled above.
        return Ok(());
    }

    pub fn spi_write(&self, addr: u32, value: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        // Grayskull doesn't have support for arc based spi read/write. The messages are
        // unpopulated, but I am explicily setting it to false out of an abundence of caution.
        let spi = super::spi::ActiveSpi::new(self, false)?;

        spi.write(self, addr, value)?;

        Ok(())
    }

    pub fn spi_read(&self, addr: u32, value: &mut [u8]) -> Result<(), Box<dyn std::error::Error>> {
        // Grayskull doesn't have support for arc based spi read/write. The messages are
        // unpopulated, but I am explicily setting it to false out of an abundence of caution.
        let spi = super::spi::ActiveSpi::new(self, false)?;

        spi.read(self, addr, value)?;

        Ok(())
    }
}

impl HlComms for Grayskull {
    fn comms_obj(&self) -> (&dyn ChipComms, &dyn ChipInterface) {
        (self.arc_if.as_ref(), self.chip_if.as_ref())
    }
}

impl HlComms for &Grayskull {
    fn comms_obj(&self) -> (&dyn ChipComms, &dyn ChipInterface) {
        (self.arc_if.as_ref(), self.chip_if.as_ref())
    }
}

fn default_status() -> InitStatus {
    InitStatus {
        comms_status: super::CommsStatus::CanCommunicate,
        arc_status: ComponentStatusInfo {
            wait_status: Box::new([WaitStatus::Waiting]),
            status: String::new(),

            start_time: std::time::Instant::now(),
            timeout: std::time::Duration::from_secs(10),
        },
        dram_status: ComponentStatusInfo::init_waiting(
            String::new(),
            std::time::Duration::from_secs(10),
            4,
        ),
        eth_status: ComponentStatusInfo::not_present(),
        cpu_status: ComponentStatusInfo::not_present(),

        init_options: InitOptions { noc_safe: false },

        unknown_state: false
    }
}

impl ChipImpl for Grayskull {
    fn update_init_state(
        &mut self,
        status: &mut InitStatus,
    ) -> Result<ChipInitResult, PlatformError> {
        if status.unknown_state {
            *status = default_status();
        }

        let comms = &mut status.comms_status;

        {
            let status = &mut status.arc_status;
            if let Some(arc_status) = status.wait_status.get_mut(0) {
                match arc_status {
                    WaitStatus::Waiting => {
                        match self.check_arc_msg_safe(5, 3) {
                            Ok(_) => *arc_status = WaitStatus::JustFinished,
                            Err(err) => match err {
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
                                        crate::error::ArcReadyError::NoAccess => {
                                            *comms = CommsStatus::CommunicationError("Failed to access ARC".to_string());
                                            return Ok(ChipInitResult::ErrorAbort);
                                        }
                                        crate::error::ArcReadyError::WatchdogTriggered
                                        | crate::error::ArcReadyError::Asleep => {
                                            *arc_status = WaitStatus::JustFinished;
                                            status.status = reason.to_string();
                                        }
                                        crate::error::ArcReadyError::BootIncomplete
                                        | crate::error::ArcReadyError::OutstandingPcieDMA
                                        | crate::error::ArcReadyError::MessageQueued(_)
                                        | crate::error::ArcReadyError::HandlingMessage(_)
                                        | crate::error::ArcReadyError::PostCodeBusy(_) => {
                                            if status.start_time.elapsed() > status.timeout {
                                                *arc_status = WaitStatus::Timeout(status.timeout);
                                            }
                                        }
                                    }
                                }

                                // The fact that this is here means that our result is too generic, for now we just ignore it.
                                PlatformError::ArcMsgError(_) => {
                                    return Ok(ChipInitResult::ErrorContinue);
                                }

                                // This is fine to hit at this stage (though it should have been already verified to not be the case).
                                // For now we just ignore it and hope that it will be resolved by the time the timeout expires...
                                PlatformError::EthernetTrainingNotComplete(_) => {
                                    return Ok(ChipInitResult::ErrorContinue);
                                }

                                // This is an "expected error" but we probably can't recover from it, so we should abort the init.
                                PlatformError::AxiError(_) => {
                                    return Ok(ChipInitResult::ErrorAbort)
                                }

                                // We don't expect to hit these cases so if we do, we should assume that something went terribly
                                // wrong and abort the init.
                                PlatformError::UnsupportedFwVersion { .. }
                                | PlatformError::WrongChipArch { .. }
                                | PlatformError::WrongChipArchs { .. }
                                | PlatformError::Generic(_, _)
                                | PlatformError::GenericError(_, _) => {
                                    return Ok(ChipInitResult::ErrorAbort)
                                }
                            },
                        }
                        {}
                    }
                    WaitStatus::JustFinished => {
                        *arc_status = WaitStatus::Done;
                    }
                    WaitStatus::Done | WaitStatus::Timeout(_) | WaitStatus::NotPresent => {}
                    _ => {}
                }
            }
        }

        {
            // TODO(drosen): Explicitly check against the telemetry info
            let status = &mut status.dram_status;
            // match status.wait_status {
            //     WaitStatus::Waiting(start) => {
            //         let timeout = std::time::Duration::from_secs(10);
            //         if let Ok(_) = self.check_arc_msg_safe(5, 3) {
            //             status.wait_status = WaitStatus::JustFinished;
            //         } else if start.elapsed() > timeout {
            //             status.wait_status = WaitStatus::Timeout(timeout);
            //         }
            //     }
            //     WaitStatus::JustFinished => {
            //         status.wait_status = WaitStatus::Done;
            //     }
            //     WaitStatus::Done | WaitStatus::Timeout(_) | WaitStatus::NotPresent => {}
            // }

            // For now assume that if ARC is good, then so is dram
            for dram_status in status.wait_status.iter_mut() {
                *dram_status = WaitStatus::Done;
            }
        }

        {
            // This is not present in grayskull.
            let _status = &mut status.eth_status;
        }

        {
            // This is not present in grayskull.
            let _status = &mut status.cpu_status;
        }

        Ok(ChipInitResult::NoError)
    }

    fn get_arch(&self) -> luwen_core::Arch {
        Arch::Grayskull
    }

    fn arc_msg(&self, msg: ArcMsgOptions) -> Result<ArcMsgOk, PlatformError> {
        let (msg_reg, return_reg) = if msg.use_second_mailbox {
            return Err(ArcMsgProtocolError::InvalidMailbox(2).into_error())?;
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

    fn get_neighbouring_chips(&self) -> Result<Vec<NeighbouringChip>, PlatformError> {
        Ok(vec![])
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn get_telemetry(&self) -> Result<super::Telemetry, crate::error::PlatformError> {
        let result = self.arc_msg(ArcMsgOptions {
            msg: ArcMsg::FwVersion(crate::arc_msg::FwType::ArcL2),
            ..Default::default()
        })?;
        let version = match result {
            ArcMsgOk::Ok { arg, .. } => arg,
            ArcMsgOk::OkNoWait => {
                unreachable!("FwVersion should always be waited on for completion")
            }
        };

        if version <= 0x01030000 {
            return Err(crate::error::PlatformError::UnsupportedFwVersion {
                version,
                required: 0x01040000,
            });
        }

        let result = self.arc_msg(ArcMsgOptions {
            msg: ArcMsg::GetSmbusTelemetryAddr,
            ..Default::default()
        })?;

        let offset = match result {
            ArcMsgOk::Ok { arg, .. } => arg,
            ArcMsgOk::OkNoWait => {
                return Err(
                    "GetSmbusTelemetryAddr should always be waited on for completion".to_string(),
                )?;
            }
        };

        let csm_offset = self.arc_if.axi_translate("ARC_CSM.DATA[0]")?;

        let telemetry_struct_offset = csm_offset.addr + (offset - 0x10000000) as u64;
        let smbus_tx_enum_version = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (0 * 4))?;
        let smbus_tx_device_id = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (1 * 4))?;
        let smbus_tx_asic_ro = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (2 * 4))?;
        let smbus_tx_asic_idd = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (3 * 4))?;
        let smbus_tx_board_id_high = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (4 * 4))?;
        let smbus_tx_board_id_low = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (5 * 4))?;
        let smbus_tx_arc0_fw_version = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (6 * 4))?;
        let smbus_tx_arc1_fw_version = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (7 * 4))?;
        let smbus_tx_arc2_fw_version = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (8 * 4))?;
        let smbus_tx_arc3_fw_version = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (9 * 4))?;
        let smbus_tx_spibootrom_fw_version = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (10 * 4))?;
        let smbus_tx_ddr_speed = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (11 * 4))?;
        let smbus_tx_ddr_status = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (12 * 4))?;
        let smbus_tx_pcie_status = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (13 * 4))?;
        let smbus_tx_faults = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (14 * 4))?;
        let smbus_tx_arc0_health = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (15 * 4))?;
        let smbus_tx_arc1_health = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (16 * 4))?;
        let smbus_tx_arc2_health = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (17 * 4))?;
        let smbus_tx_arc3_health = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (18 * 4))?;
        let smbus_tx_fan_speed = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (19 * 4))?;
        let smbus_tx_aiclk = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (20 * 4))?;
        let smbus_tx_axiclk = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (21 * 4))?;
        let smbus_tx_arcclk = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (22 * 4))?;
        let smbus_tx_throttler = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (23 * 4))?;
        let smbus_tx_vcore = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (24 * 4))?;
        let smbus_tx_asic_temperature = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (25 * 4))?;
        let smbus_tx_vreg_temperature = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (26 * 4))?;
        let smbus_tx_tdp = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (27 * 4))?;
        let smbus_tx_tdc = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (28 * 4))?;
        let smbus_tx_vdd_limits = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (29 * 4))?;
        let smbus_tx_thm_limits = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (30 * 4))?;
        let smbus_tx_wh_fw_date = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (31 * 4))?;
        let smbus_tx_asic_tmon0 = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (32 * 4))?;
        let smbus_tx_asic_tmon1 = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (33 * 4))?;
        let smbus_tx_asic_power = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (34 * 4))?;

        let smbus_tx_aux_status = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (35 * 4))?;
        let smbus_tx_boot_date = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (36 * 4))?;
        let smbus_tx_rt_seconds = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (37 * 4))?;
        let smbus_tx_tt_flash_version = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (38 * 4))?;

        // let board_id_high =
        //     self.arc_if
        //         .axi_read32(&self.chip_if, telemetry_struct_offset + (4 * 4))? as u64;
        // let board_id_low =
        //     self.arc_if
        //         .axi_read32(&self.chip_if, telemetry_struct_offset + (5 * 4))? as u64;

        Ok(super::Telemetry {
            board_id: ((smbus_tx_board_id_high as u64) << 32) | (smbus_tx_board_id_low as u64),
            smbus_tx_enum_version,
            smbus_tx_device_id,
            smbus_tx_asic_ro,
            smbus_tx_asic_idd,
            smbus_tx_board_id_high,
            smbus_tx_board_id_low,
            smbus_tx_arc0_fw_version,
            smbus_tx_arc1_fw_version,
            smbus_tx_arc2_fw_version,
            smbus_tx_arc3_fw_version,
            smbus_tx_spibootrom_fw_version,
            smbus_tx_ddr_speed: Some(smbus_tx_ddr_speed),
            smbus_tx_ddr_status,
            smbus_tx_pcie_status,
            smbus_tx_faults,
            smbus_tx_arc0_health,
            smbus_tx_arc1_health,
            smbus_tx_arc2_health,
            smbus_tx_arc3_health,
            smbus_tx_fan_speed,
            smbus_tx_aiclk,
            smbus_tx_axiclk,
            smbus_tx_arcclk,
            smbus_tx_throttler,
            smbus_tx_vcore,
            smbus_tx_asic_temperature,
            smbus_tx_vreg_temperature,
            smbus_tx_tdp,
            smbus_tx_tdc,
            smbus_tx_vdd_limits,
            smbus_tx_thm_limits,
            smbus_tx_wh_fw_date,
            smbus_tx_asic_tmon0,
            smbus_tx_asic_tmon1,
            smbus_tx_asic_power: Some(smbus_tx_asic_power),
            smbus_tx_aux_status: Some(smbus_tx_aux_status),
            smbus_tx_boot_date,
            smbus_tx_rt_seconds,
            smbus_tx_tt_flash_version,
            ..Default::default()
        })
    }

    fn get_device_info(&self) -> Result<Option<crate::DeviceInfo>, PlatformError> {
        Ok(self.chip_if.get_device_info()?)
    }
}
