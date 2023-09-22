use std::sync::Arc;

use luwen_core::Arch;

use crate::{
    arc_msg::{ArcMsgAddr, ArcMsgError, ArcMsgOk, ArcMsgProtocolError},
    ArcMsg, ChipImpl,
};

use super::{ArcMsgOptions, ChipComms, ChipInterface, HlComms, NeighbouringChip};

#[derive(Clone)]
pub struct Grayskull {
    pub chip_if: Arc<dyn ChipInterface + Send + Sync>,
    pub arc_if: Arc<dyn ChipComms + Send + Sync>,

    pub arc_addrs: ArcMsgAddr,
}

impl Grayskull {
    pub fn get_if<T: ChipInterface>(&self) -> Option<&T> {
        (&self.chip_if as &dyn std::any::Any).downcast_ref::<T>()
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
    fn init(&self) {}

    fn get_arch(&self) -> luwen_core::Arch {
        Arch::Grayskull
    }

    fn arc_msg(&self, msg: ArcMsgOptions) -> Result<ArcMsgOk, ArcMsgError> {
        let (msg_reg, return_reg) = if msg.use_second_mailbox {
            return Err(ArcMsgProtocolError::InvalidMailbox(2).into_error());
        } else {
            (5, 3)
        };

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

    fn get_neighbouring_chips(&self) -> Vec<NeighbouringChip> {
        vec![]
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
                .axi_read32(&self.chip_if, telemetry_struct_offset + (4 * 4)) as u64;
        let board_id_low =
            self.arc_if
                .axi_read32(&self.chip_if, telemetry_struct_offset + (5 * 4)) as u64;

        Ok(super::Telemetry {
            board_id: (board_id_high << 32) | board_id_low,
        })
    }

    fn get_device_info(&self) -> Option<crate::DeviceInfo> {
        self.chip_if.get_device_info()
    }
}
