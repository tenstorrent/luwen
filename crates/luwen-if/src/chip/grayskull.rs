use std::sync::Arc;

use luwen_core::Arch;

use crate::{
    arc_msg::{ArcMsgAddr, ArcMsgOk, ArcMsgProtocolError},
    error::PlatformError,
    ArcMsg, ChipImpl, chip::HlCommsInterface,
};

use super::{ArcMsgOptions, ChipComms, ChipInterface, HlComms, NeighbouringChip, InitStatus, WaitStatus, StatusInfo};

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

    fn check_arg_msg_safe(&self, msg_reg: u64, _return_reg: u64) -> Result<(), PlatformError> {
        const POST_CODE_INIT_DONE: u32 = 0xC0DE0001;
        const _POST_CODE_ARC_MSG_HANDLE_START: u32 = 0xC0DE0030;

        let s5 = self.axi_sread32(format!("ARC_RESET.SCRATCH[{msg_reg}]"))?;
        let pc = self.axi_sread32("ARC_RESET.POST_CODE")?;
        let dma = self.axi_sread32("ARC_CSM.ARC_PCIE_DMA_REQUEST.trigger")?;

        if pc == 0xFFFFFFFF {
            return Err(ArcMsgProtocolError::UnsafeToSendArcMsg(
                "scratch register access failed".to_string(),
            )
            .into_error())?;
        }

        if s5 == 0xDEADC0DE {
            return Err(ArcMsgProtocolError::UnsafeToSendArcMsg(
                "ARC watchdog has triggered".to_string(),
            )
            .into_error())?;
        }

        // Still booting and it will later wipe SCRATCH[5/2].
        if s5 == 0x00000060 || pc == 0x11110000 {
            return Err(ArcMsgProtocolError::UnsafeToSendArcMsg(
                "ARC FW has not yet booted".to_string(),
            )
            .into_error())?;
        }

        if s5 == 0x0000AA00 || s5 == ArcMsg::ArcGoToSleep.msg_code() as u32 {
            return Err(
                ArcMsgProtocolError::UnsafeToSendArcMsg("ARC is asleep".to_string()).into_error(),
            )?;
        }

        // PCIE DMA writes SCRATCH[5] on exit, so it's not safe.
        // Also we assume FW is hung if we see this state.
        // (The former is only relevant when msg_reg==5, but the latter is always relevant.)
        if dma != 0 {
            return Err(ArcMsgProtocolError::UnsafeToSendArcMsg(
                "there is an outstanding PCIE DMA request".to_string(),
            )
            .into_error())?;
        }

        if s5 & 0xFFFFFF00 == 0x0000AA00 {
            let message_id = s5 & 0xFF;
            return Err(ArcMsgProtocolError::UnsafeToSendArcMsg(format!(
                "another message is queued (0x{message_id:02x})"
            ))
            .into_error())?;
        }

        if s5 & 0xFF00FFFF == 0xAA000000 {
            let message_id = (s5 >> 16) & 0xFF;
            return Err(ArcMsgProtocolError::UnsafeToSendArcMsg(format!(
                "another message is being procesed (0x{message_id:02x})"
            ))
            .into_error())?;
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
                return Err(ArcMsgProtocolError::UnsafeToSendArcMsg(format!(
                    "post code 0x{pc:08x} indicates ARC is not ready"
                ))
                .into_error())?;
            }
        }

        // We should never get here, every case should be handled above.
        return Ok(());
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

impl ChipImpl for Grayskull {
    fn is_inititalized(&self) -> Result<InitStatus, PlatformError> {
        let mut status = InitStatus {
            arc_status: StatusInfo {
                total: 1,
                ..Default::default()
            },
            dram_status: StatusInfo {
                total: 4,
                ..Default::default() },
            eth_status: StatusInfo::not_present(),
            cpu_status: StatusInfo::not_present(),
        };
        self.update_init_state(&mut status)?;

        Ok(status)
    }

    fn update_init_state(&self, status: &mut InitStatus) -> Result<(), PlatformError> {
        {
            let status = &mut status.arc_status;
            match status.wait_status {
                WaitStatus::Waiting(start) => {
                    let timeout = std::time::Duration::from_secs(10);
                    if let Ok(_) = self.check_arg_msg_safe(5, 3) {
                        status.wait_status = WaitStatus::JustFinished;
                    } else if start.elapsed() > timeout {
                        status.wait_status = WaitStatus::Timeout(timeout);
                    }
                }
                WaitStatus::JustFinished => {
                    status.wait_status = WaitStatus::Done;
                }
                WaitStatus::Done | WaitStatus::Timeout(_) |
                WaitStatus::NotPresent => {}
            }
        }

        {
            let status = &mut status.dram_status;
            match status.wait_status {
                WaitStatus::Waiting(start) => {
                    let timeout = std::time::Duration::from_secs(10);
                    if let Ok(_) = self.check_arg_msg_safe(5, 3) {
                        status.wait_status = WaitStatus::JustFinished;
                    } else if start.elapsed() > timeout {
                        status.wait_status = WaitStatus::Timeout(timeout);
                    }
                }
                WaitStatus::JustFinished => {
                    status.wait_status = WaitStatus::Done;
                }
                WaitStatus::Done  | WaitStatus::Timeout(_) |
                WaitStatus::NotPresent => {}
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

        Ok(())
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

        self.check_arg_msg_safe(msg_reg, return_reg)?;

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
            msg: ArcMsg::FwVersion,
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
                unreachable!("GetSmbusTelemetryAddr should always be waited on for completion")
            }
        };

        let csm_offset = self.arc_if.axi_translate("ARC_CSM.DATA[0]")?;

        let telemetry_struct_offset = csm_offset.addr + (offset - 0x10000000) as u64;

        let board_id_high =
            self.arc_if
                .axi_read32(&self.chip_if, telemetry_struct_offset + (4 * 4))? as u64;
        let board_id_low =
            self.arc_if
                .axi_read32(&self.chip_if, telemetry_struct_offset + (5 * 4))? as u64;

        Ok(super::Telemetry {
            board_id: (board_id_high << 32) | board_id_low,
        })
    }

    fn get_device_info(&self) -> Result<Option<crate::DeviceInfo>, PlatformError> {
        Ok(self.chip_if.get_device_info()?)
    }
}
