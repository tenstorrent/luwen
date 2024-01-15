// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use crate::{
    arc_msg::{ArcMsgAddr, ArcMsgOk},
    chip::{
        communication::{
            chip_comms::{load_axi_table, ChipComms},
            chip_interface::ChipInterface,
        },
        hl_comms::HlCommsInterface,
    },
    error::PlatformError,
    ArcMsg, ChipImpl, IntoChip,
};

use super::{
    eth_addr::EthAddr,
    hl_comms::HlComms,
    remote::{EthAddresses, RemoteArcIf},
    ArcMsgOptions, ChipInitResult, InitStatus, NeighbouringChip, StatusInfo, WaitStatus,
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
        use_arc_for_spi: bool,
        arc_if: CC,
        chip_if: CI,
    ) -> Result<Self, PlatformError> {
        // let mut version = [0; 4];
        // arc_if.axi_read(&chip_if, 0x0, &mut version);
        // let version = u32::from_le_bytes(version);
        let _version = 0x0;

        let mut fw_version = [0; 4];
        arc_if.noc_read(&chip_if, 0, 1, 0, 0x210, &mut fw_version)?;
        let fw_version = u32::from_le_bytes(fw_version);

        let output = Wormhole {
            chip_if: Arc::new(chip_if),

            is_remote,
            use_arc_for_spi,

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

    fn check_arg_msg_safe(&self, msg_reg: u64, _return_reg: u64) -> Result<(), PlatformError> {
        const POST_CODE_INIT_DONE: u32 = 0xC0DE0001;
        const _POST_CODE_ARC_MSG_HANDLE_START: u32 = 0xC0DE0030;
        const POST_CODE_ARC_MSG_HANDLE_DONE: u32 = 0xC0DE003F;
        const POST_CODE_ARC_TIME_LAST: u32 = 0xC0DE007F;

        let s5 = self.axi_sread32(format!("ARC_RESET.SCRATCH[{msg_reg}]"))?;
        let pc = self.axi_sread32("ARC_RESET.POST_CODE")?;
        let dma = self.axi_sread32("ARC_CSM.ARC_PCIE_DMA_REQUEST.trigger")?;

        if pc == 0xFFFFFFFF {
            return Err(PlatformError::ArcNotReady(
                "scratch register access failed".to_string(),
            ))?;
        }

        if s5 == 0xDEADC0DE {
            return Err(PlatformError::ArcNotReady(
                "ARC watchdog has triggered".to_string(),
            ))?;
        }

        // Still booting and it will later wipe SCRATCH[5/2].
        if s5 == 0x00000060 || pc == 0x11110000 {
            return Err(PlatformError::ArcNotReady(
                "ARC FW has not yet booted".to_string(),
            ))?;
        }

        if s5 == 0x0000AA00 || s5 == ArcMsg::ArcGoToSleep.msg_code() as u32 {
            return Err(PlatformError::ArcNotReady("ARC is asleep".to_string()))?;
        }

        // PCIE DMA writes SCRATCH[5] on exit, so it's not safe.
        // Also we assume FW is hung if we see this state.
        // (The former is only relevant when msg_reg==5, but the latter is always relevant.)
        if dma != 0 {
            return Err(PlatformError::ArcNotReady(
                "there is an outstanding PCIE DMA request".to_string(),
            ))?;
        }

        if s5 & 0xFFFFFF00 == 0x0000AA00 {
            let message_id = s5 & 0xFF;
            return Err(PlatformError::ArcNotReady(format!(
                "another message is queued (0x{message_id:02x})"
            )))?;
        }

        if s5 & 0xFF00FFFF == 0xAA000000 {
            let message_id = (s5 >> 16) & 0xFF;
            return Err(PlatformError::ArcNotReady(format!(
                "another message is being procesed (0x{message_id:02x})"
            )))?;
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
                return Err(PlatformError::ArcNotReady(format!(
                    "post code 0x{pc:08x} indicates ARC is not ready"
                )))?;
            }
        }

        // We should never get here, every case should be handled above.
        return Ok(());
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

impl ChipImpl for Wormhole {
    fn is_inititalized(&self) -> Result<InitStatus, PlatformError> {
        let mut status = InitStatus {
            arc_status: StatusInfo {
                total: 1,
                ..Default::default()
            },
            dram_status: StatusInfo {
                total: 4,
                ..Default::default()
            },
            eth_status: StatusInfo {
                total: 16,
                ..Default::default()
            },
            cpu_status: StatusInfo::not_present(),
        };
        self.update_init_state(&mut status)?;

        Ok(status)
    }

    fn update_init_state(&self, status: &mut InitStatus) -> Result<ChipInitResult, PlatformError> {
        {
            let status = &mut status.arc_status;
            match status.wait_status {
                WaitStatus::Waiting(start) => {
                    let timeout = std::time::Duration::from_secs(300);
                    match self.check_arg_msg_safe(5, 3) {
                        Ok(_) => status.wait_status = WaitStatus::JustFinished,
                        Err(err) => match err {
                            PlatformError::ArcNotReady(_) => {
                                if start.elapsed() > timeout {
                                    status.wait_status = WaitStatus::Timeout(timeout)
                                }
                            }
                            PlatformError::AxiError(_) => todo!(),

                            // If we hit these, something has gone terribly wrong. We will therefore abort...
                            PlatformError::WrongChipArch {
                                actual,
                                expected,
                                backtrace,
                            } => todo!(),
                            PlatformError::UnsupportedFwVersion { version, required } => todo!(),
                            PlatformError::ArcMsgError(_) => todo!(),
                            PlatformError::EthernetTrainingNotComplete(_) => todo!(),
                            PlatformError::Generic(_, _) => todo!(),
                            PlatformError::GenericError(_, _) => todo!(),
                        },
                    }
                    {}
                }
                WaitStatus::JustFinished => {
                    status.wait_status = WaitStatus::Done;
                }
                WaitStatus::Done | WaitStatus::Timeout(_) | WaitStatus::NotPresent => {}
            }
        }

        {
            // TODO(drosen): Explicitly check against the telemetry info
            let status = &mut status.dram_status;
            // match status.wait_status {
            //     WaitStatus::Waiting(start) => {
            //         let timeout = std::time::Duration::from_secs(10);
            //         if let Ok(_) = self.check_arg_msg_safe(5, 3) {
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
        }

        {
            let status = &mut status.eth_status;
            match status.wait_status {
                WaitStatus::Waiting(start) => {
                    let timeout = std::time::Duration::from_secs(300);
                    if let Ok(_) = self.check_ethernet_training_complete() {
                        status.wait_status = WaitStatus::JustFinished;
                    } else if start.elapsed() > timeout {
                        status.wait_status = WaitStatus::Timeout(timeout);
                    }
                }
                WaitStatus::JustFinished => {
                    status.wait_status = WaitStatus::Done;
                }
                WaitStatus::Done | WaitStatus::Timeout(_) | WaitStatus::NotPresent => {}
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

    fn get_neighbouring_chips(&self) -> Result<Vec<NeighbouringChip>, crate::error::PlatformError> {
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
            )?;

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
            )?;
            let remote_rack_x = remote_id & 0xFF;
            let remote_rack_y = (remote_id >> 8) & 0xFF;

            let remote_id = self.noc_read32(
                0,
                eth_x,
                eth_y,
                self.eth_addres.node_info + (4 * SHELF_OFFSET),
            )?;
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

        Ok(output)
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
        let smbus_tx_eth_fw_version = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (11 * 4))?;
        let smbus_tx_m3_bl_fw_version = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (12 * 4))?;
        let smbus_tx_m3_app_fw_version = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (13 * 4))?;
        let smbus_tx_ddr_status = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (14 * 4))?;
        let smbus_tx_eth_status0 = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (15 * 4))?;
        let smbus_tx_eth_status1 = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (16 * 4))?;
        let smbus_tx_pcie_status = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (17 * 4))?;
        let smbus_tx_faults = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (18 * 4))?;
        let smbus_tx_arc0_health = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (19 * 4))?;
        let smbus_tx_arc1_health = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (20 * 4))?;
        let smbus_tx_arc2_health = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (21 * 4))?;
        let smbus_tx_arc3_health = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (22 * 4))?;
        let smbus_tx_fan_speed = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (23 * 4))?;
        let smbus_tx_aiclk = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (24 * 4))?;
        let smbus_tx_axiclk = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (25 * 4))?;
        let smbus_tx_arcclk = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (26 * 4))?;
        let smbus_tx_throttler = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (27 * 4))?;
        let smbus_tx_vcore = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (28 * 4))?;
        let smbus_tx_asic_temperature = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (29 * 4))?;
        let smbus_tx_vreg_temperature = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (30 * 4))?;
        let smbus_tx_board_temperature = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (31 * 4))?;
        let smbus_tx_tdp = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (32 * 4))?;
        let smbus_tx_tdc = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (33 * 4))?;
        let smbus_tx_vdd_limits = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (34 * 4))?;
        let smbus_tx_thm_limits = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (35 * 4))?;
        let smbus_tx_wh_fw_date = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (36 * 4))?;
        let smbus_tx_asic_tmon0 = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (37 * 4))?;
        let smbus_tx_asic_tmon1 = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (38 * 4))?;
        let smbus_tx_mvddq_power = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (39 * 4))?;
        let smbus_tx_gddr_train_temp0 = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (40 * 4))?;
        let smbus_tx_gddr_train_temp1 = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (41 * 4))?;
        let smbus_tx_boot_date = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (42 * 4))?;
        let smbus_tx_rt_seconds = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (43 * 4))?;
        let smbus_tx_eth_debug_status0 = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (44 * 4))?;
        let smbus_tx_eth_debug_status1 = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (45 * 4))?;
        let smbus_tx_tt_flash_version = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + (46 * 4))?;

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
            smbus_tx_eth_fw_version,
            smbus_tx_m3_bl_fw_version,
            smbus_tx_m3_app_fw_version,
            smbus_tx_ddr_status,
            smbus_tx_eth_status0,
            smbus_tx_eth_status1,
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
            smbus_tx_board_temperature,
            smbus_tx_tdp,
            smbus_tx_tdc,
            smbus_tx_vdd_limits,
            smbus_tx_thm_limits,
            smbus_tx_wh_fw_date,
            smbus_tx_asic_tmon0,
            smbus_tx_asic_tmon1,
            smbus_tx_mvddq_power,
            smbus_tx_gddr_train_temp0,
            smbus_tx_gddr_train_temp1,
            smbus_tx_boot_date,
            smbus_tx_rt_seconds,
            smbus_tx_eth_debug_status0,
            smbus_tx_eth_debug_status1,
            smbus_tx_tt_flash_version,
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
