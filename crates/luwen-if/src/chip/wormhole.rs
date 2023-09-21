use std::sync::Arc;

use crate::{
    arc_msg::{ArcMsgAddr, ArcMsgError, ArcMsgOk, ArcMsgProtocolError},
    chip::communication::{
        chip_comms::{load_axi_table, ChipComms},
        chip_interface::ChipInterface,
    },
    error::PlatformError,
    ArcMsg, ChipImpl, IntoChip,
};

use super::{
    eth_addr::EthAddr,
    hl_comms::HlComms,
    remote::{EthAddresses, RemoteArcIf},
    ArcMsgOptions, AxiError, NeighbouringChip,
};

/// Implementation of the interface for a Wormhole
/// both the local and remote Wormhole chips are represented by this struct
#[derive(Clone)]
pub struct Wormhole {
    pub chip_if: Arc<dyn ChipInterface + Send + Sync>,
    pub arc_if: Arc<dyn ChipComms + Send + Sync>,

    pub is_remote: bool,

    pub arc_addrs: ArcMsgAddr,
    pub eth_addres: EthAddresses,
    pub eth_locations: [(u8, u8); 16],
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

impl Wormhole {
    pub(crate) fn init<
        CC: ChipComms + Send + Sync + 'static,
        CI: ChipInterface + Send + Sync + 'static,
    >(
        is_remote: bool,
        arc_if: CC,
        chip_if: CI,
    ) -> Result<Self, AxiError> {
        // let mut version = [0; 4];
        // arc_if.axi_read(&chip_if, 0x0, &mut version);
        // let version = u32::from_le_bytes(version);
        let _version = 0x0;

        let mut fw_version = [0; 4];
        arc_if.noc_read(&chip_if, 0, 1, 0, 0x210, &mut fw_version);
        let fw_version = u32::from_le_bytes(fw_version);

        let output = Wormhole {
            chip_if: Arc::new(chip_if),

            is_remote,

            arc_addrs: ArcMsgAddr::try_from(&arc_if as &dyn ChipComms)?,

            arc_if: Arc::new(arc_if),

            eth_addres: EthAddresses::new(fw_version),
            eth_locations: [
                (9, 0),
                (1, 0),
                (8, 0),
                (2, 0),
                (7, 0),
                (3, 0),
                (6, 0),
                (4, 0),
                (9, 6),
                (1, 6),
                (8, 6),
                (2, 6),
                (7, 6),
                (3, 6),
                (6, 6),
                (4, 6),
            ],
        };

        Ok(output)
    }

    pub fn open_remote(&self, addr: impl IntoChip<EthAddr>) -> Result<Wormhole, AxiError> {
        let arc_if = RemoteArcIf {
            addr: addr.cinto(&self.arc_if, &self.chip_if).unwrap(),
            axi_data: Some(load_axi_table("wormhole-axi-noc.bin", 0)),
        };

        Self::init(true, arc_if, self.chip_if.clone())
    }

    // fn check_dram_trained(&self) {
    //     let pc = self.axi_sread32("ARC_RESET.POST_CODE")?;

    //     0x29
    // }

    fn check_arg_msg_safe(&self, msg_reg: u64, _return_reg: u64) -> Result<(), ArcMsgError> {
        const POST_CODE_INIT_DONE: u32 = 0xC0DE0001;
        const _POST_CODE_ARC_MSG_HANDLE_START: u32 = 0xC0DE0030;
        const POST_CODE_ARC_MSG_HANDLE_DONE: u32 = 0xC0DE003F;
        const POST_CODE_ARC_TIME_LAST: u32 = 0xC0DE007F;

        let s5 = self.axi_sread32(format!("ARC_RESET.SCRATCH[{msg_reg}]"))?;
        let pc = self.axi_sread32("ARC_RESET.POST_CODE")?;
        let dma = self.axi_sread32("ARC_CSM.ARC_PCIE_DMA_REQUEST.trigger")?;

        if pc == 0xFFFFFFFF {
            return Err(ArcMsgProtocolError::UnsafeToSendArcMsg(
                "scratch register access failed".to_string(),
            )
            .into_error());
        }

        if s5 == 0xDEADC0DE {
            return Err(ArcMsgProtocolError::UnsafeToSendArcMsg(
                "ARC watchdog has triggered".to_string(),
            )
            .into_error());
        }

        // Still booting and it will later wipe SCRATCH[5/2].
        if s5 == 0x00000060 || pc == 0x11110000 {
            return Err(ArcMsgProtocolError::UnsafeToSendArcMsg(
                "ARC FW has not yet booted".to_string(),
            )
            .into_error());
        }

        if s5 == 0x0000AA00 || s5 == ArcMsg::ArcGoToSleep.msg_code() as u32 {
            return Err(
                ArcMsgProtocolError::UnsafeToSendArcMsg("ARC is asleep".to_string()).into_error(),
            );
        }

        // PCIE DMA writes SCRATCH[5] on exit, so it's not safe.
        // Also we assume FW is hung if we see this state.
        // (The former is only relevant when msg_reg==5, but the latter is always relevant.)
        if dma != 0 {
            return Err(ArcMsgProtocolError::UnsafeToSendArcMsg(
                "there is an outstanding PCIE DMA request".to_string(),
            )
            .into_error());
        }

        if s5 & 0xFFFFFF00 == 0x0000AA00 {
            let message_id = s5 & 0xFF;
            return Err(ArcMsgProtocolError::UnsafeToSendArcMsg(format!(
                "another message is queued (0x{message_id:02x})"
            ))
            .into_error());
        }

        if s5 & 0xFF00FFFF == 0xAA000000 {
            let message_id = (s5 >> 16) & 0xFF;
            return Err(ArcMsgProtocolError::UnsafeToSendArcMsg(format!(
                "another message is being procesed (0x{message_id:02x})"
            ))
            .into_error());
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
                || (POST_CODE_ARC_MSG_HANDLE_DONE <= pc && pc <= POST_CODE_ARC_TIME_LAST);
            if pc_idle {
                return Ok(());
            } else {
                return Err(ArcMsgProtocolError::UnsafeToSendArcMsg(format!(
                    "post code 0x{pc:08x} indicates ARC is not ready"
                ))
                .into_error());
            }
        }

        // We should never get here, every case should be handled above.
        return Ok(());
    }
}

impl ChipImpl for Wormhole {
    fn init(&self) {
        while self.check_arg_msg_safe(5, 3).is_err() {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    fn get_arch(&self) -> luwen_core::Arch {
        luwen_core::Arch::Wormhole
    }

    fn arc_msg(&self, msg: ArcMsgOptions) -> Result<ArcMsgOk, ArcMsgError> {
        let (msg_reg, return_reg) = if msg.use_second_mailbox {
            (2, 4)
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

    fn get_neighbouring_chips(&self) -> Vec<NeighbouringChip> {
        const ETH_UNKNOWN: u32 = 0;
        const ETH_UNCONNECTED: u32 = 1;

        const SHELF_OFFSET: u64 = 9;
        const RACK_OFFSET: u64 = 10;

        let mut output = Vec::with_capacity(self.eth_locations.len());

        for (eth_id, (eth_x, eth_y)) in self.eth_locations.iter().copied().enumerate() {
            let port_status = self.arc_if.noc_read32(
                &self.chip_if,
                0,
                eth_x,
                eth_y,
                self.eth_addres.eth_conn_info + (eth_id as u64 * 4),
            );

            if port_status == ETH_UNCONNECTED || port_status == ETH_UNKNOWN {
                continue;
            }

            // Decode the remote eth_addr for our erisc core
            // This can be used to build a map of the full mesh
            let remote_id = self.noc_read32(
                0,
                eth_x,
                eth_y,
                self.eth_addres.node_info + (4 * RACK_OFFSET),
            );
            let remote_rack_x = remote_id & 0xFF;
            let remote_rack_y = (remote_id >> 8) & 0xFF;

            let remote_id = self.noc_read32(
                0,
                eth_x,
                eth_y,
                self.eth_addres.node_info + (4 * SHELF_OFFSET),
            );
            let remote_shelf_x = (remote_id >> 16) & 0x3F;
            let remote_shelf_y = (remote_id >> 22) & 0x3F;

            let remote_noc_x = (remote_id >> 4) & 0x3F;
            let remote_noc_y = (remote_id >> 10) & 0x3F;

            output.push(NeighbouringChip {
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

        output
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn get_telemetry(&self) -> Result<super::Telemetry, PlatformError> {
        let result = self.arc_msg(ArcMsgOptions {
            msg: ArcMsg::GetSmbusTelemetryAddr,
            ..Default::default()
        })?;

        let offset = match result {
            ArcMsgOk::Ok { arg, .. } => arg,
            ArcMsgOk::OkNoWait => todo!(),
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
        if self.is_remote {
            None
        } else {
            self.chip_if.get_device_info()
        }
    }
}
