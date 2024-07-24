// SPDX-FileCopyrightText: © 2024 Tenstorrent Inc.
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
    ArcMsgOptions, AxiData, ChipInitResult, CommsStatus, InitStatus, NeighbouringChip,
};

pub mod message;
#[macro_use]
pub mod telemetry_tags;
use crate::chip::blackhole::telemetry_tags::TelemetryTags;

// pub use telemetry_tags::telemetry_tags_to_u32;

fn u64_from_slice(data: &[u8]) -> u64 {
    let mut output = 0;
    for i in data.iter().rev().copied() {
        output <<= 8;
        output |= i as u64;
    }

    output
}

fn u32_from_slice(data: &[u8], index: u8) -> u32 {
    let mut output = 0;
    let index = index * 4;
    let data_chunk = &data[index as usize..(index + 4) as usize];
    for i in data_chunk.iter().rev().copied() {
        output <<= 8;
        output |= i as u32;
    }
    output
}

#[derive(Clone)]
pub struct Blackhole {
    pub chip_if: Arc<dyn ChipInterface + Send + Sync>,
    pub arc_if: Arc<dyn ChipComms + Send + Sync>,

    pub message_queue: message::MessageQueue<8>,

    pub eth_locations: [EthCore; 14],
    pub eth_addrs: EthAddresses,

    telemetry_addr: AxiData,
    telemetry_struct_addr: AxiData,
}

impl HlComms for Blackhole {
    fn comms_obj(&self) -> (&dyn ChipComms, &dyn ChipInterface) {
        (self.arc_if.as_ref(), self.chip_if.as_ref())
    }
}

impl HlComms for &Blackhole {
    fn comms_obj(&self) -> (&dyn ChipComms, &dyn ChipInterface) {
        (self.arc_if.as_ref(), self.chip_if.as_ref())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct L2Core {
    pub x: u8,
    pub y: u8,
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

impl Blackhole {
    pub(crate) fn init<
        CC: ChipComms + Send + Sync + 'static,
        CI: ChipInterface + Send + Sync + 'static,
    >(
        arc_if: CC,
        chip_if: CI,
    ) -> Result<Self, PlatformError> {
        // let mut version = [0; 4];
        // arc_if.axi_read(&chip_if, 0x0, &mut version);
        // let version = u32::from_le_bytes(version);
        let _version = 0x0;

        let message_queue_info_address =
            arc_if.axi_sread32(&chip_if, "arc_ss.reset_unit.SCRATCH_RAM[11]")? as u64;
        let queue_base = arc_if.axi_read32(&chip_if, message_queue_info_address)?;
        let queue_sizing = arc_if.axi_read32(&chip_if, message_queue_info_address + 4)?;
        let queue_size = queue_sizing & 0xFF;
        let queue_count = (queue_sizing >> 8) & 0xFF;

        let output = Blackhole {
            chip_if: Arc::new(chip_if),

            message_queue: message::MessageQueue {
                header_size: 8,
                entry_size: 8,
                queue_base: queue_base as u64,
                queue_size,
                queue_count,
                fw_int: arc_if.axi_translate("arc_ss.reset_unit.ARC_MISC_CNTL.irq0_trig")?,
            },

            eth_addrs: EthAddresses::default(),

            telemetry_addr: arc_if.axi_translate("arc_ss.reset_unit.SCRATCH_RAM[12]")?,
            telemetry_struct_addr: arc_if.axi_translate("arc_ss.reset_unit.SCRATCH_RAM[13]")?,

            arc_if: Arc::new(arc_if),

            eth_locations: [
                EthCore {
                    x: 1,
                    y: 1,
                    ..Default::default()
                },
                EthCore {
                    x: 16,
                    y: 1,
                    ..Default::default()
                },
                EthCore {
                    x: 2,
                    y: 1,
                    ..Default::default()
                },
                EthCore {
                    x: 15,
                    y: 1,
                    ..Default::default()
                },
                EthCore {
                    x: 3,
                    y: 1,
                    ..Default::default()
                },
                EthCore {
                    x: 14,
                    y: 1,
                    ..Default::default()
                },
                EthCore {
                    x: 4,
                    y: 1,
                    ..Default::default()
                },
                EthCore {
                    x: 13,
                    y: 1,
                    ..Default::default()
                },
                EthCore {
                    x: 5,
                    y: 1,
                    ..Default::default()
                },
                EthCore {
                    x: 12,
                    y: 1,
                    ..Default::default()
                },
                EthCore {
                    x: 6,
                    y: 1,
                    ..Default::default()
                },
                EthCore {
                    x: 11,
                    y: 1,
                    ..Default::default()
                },
                EthCore {
                    x: 7,
                    y: 1,
                    ..Default::default()
                },
                EthCore {
                    x: 10,
                    y: 1,
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

    fn bh_arc_msg(
        &self,
        code: u16,
        data: &[u32],
        timeout: Option<std::time::Duration>,
    ) -> Result<(u8, u16, [u32; 7]), PlatformError> {
        let mut request = [0; 8];
        request[0] = code as u32;
        for (i, o) in data.iter().zip(request[1..].iter_mut()) {
            *o = *i;
        }

        let timeout = timeout.unwrap_or(std::time::Duration::from_millis(500));

        let response = self
            .message_queue
            .send_message(&self, 2, request, timeout)?;
        let status = (response[0] & 0xFF) as u8;
        let rc = (response[0] >> 16) as u16;

        if status < 240 {
            let data = [
                response[1],
                response[2],
                response[3],
                response[4],
                response[5],
                response[6],
                response[7],
            ];
            Ok((status, rc, data))
        } else if status == 0xFF {
            Err(PlatformError::ArcMsgError(
                crate::ArcMsgError::ProtocolError {
                    source: crate::ArcMsgProtocolError::MsgNotRecognized(code),
                    backtrace: BtWrapper::capture(),
                },
            ))
        } else {
            Err(PlatformError::ArcMsgError(
                crate::ArcMsgError::ProtocolError {
                    source: crate::ArcMsgProtocolError::UnknownErrorCode(status),
                    backtrace: BtWrapper::capture(),
                },
            ))
        }
    }

    pub fn spi_write(&self, mut addr: u32, value: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        unimplemented!("No SPI write implementation yet");
    }

    pub fn spi_read(
        &self,
        mut addr: u32,
        value: &mut [u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        unimplemented!("No SPI read implementation yet");
    }

    pub fn get_local_chip_coord(&self) -> Result<EthAddr, PlatformError> {
        Ok(EthAddr {
            rack_x: 0,
            rack_y: 0,
            shelf_x: 0,
            shelf_y: 0,
        })
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
            8,
        ),
        eth_status: ComponentStatusInfo::init_waiting(
            "ETH".to_string(),
            std::time::Duration::from_secs(15 * 60),
            14,
        ),
        cpu_status: ComponentStatusInfo::init_waiting(
            "CPU".to_string(),
            std::time::Duration::from_secs(60),
            4,
        ),

        init_options: InitOptions { noc_safe: false },

        unknown_state: false,
    }
}

impl ChipImpl for Blackhole {
    // For now just assume the chip is fully working
    // will implement proper status checking later
    fn update_init_state(
        &mut self,
        status: &mut InitStatus,
    ) -> Result<ChipInitResult, PlatformError> {
        if status.unknown_state {
            let init_options = std::mem::take(&mut status.init_options);
            *status = default_status();
            status.init_options = init_options;
        }

        {
            let comms = &mut status.comms_status;
            *comms = match self.axi_sread32("arc_ss.reset_unit.SCRATCH_0") {
                Ok(_) => CommsStatus::CanCommunicate,
                Err(err) => CommsStatus::CommunicationError(err.to_string()),
            }
        }

        {
            let status = &mut status.arc_status;
            for s in status.wait_status.iter_mut() {
                *s = WaitStatus::Done;
            }
        }

        {
            let status = &mut status.dram_status;
            for s in status.wait_status.iter_mut() {
                *s = WaitStatus::Done;
            }
        }

        {
            let status = &mut status.eth_status;
            for s in status.wait_status.iter_mut() {
                *s = WaitStatus::Done;
            }
        }

        {
            let status = &mut status.cpu_status;
            for s in status.wait_status.iter_mut() {
                *s = WaitStatus::Done;
            }
        }

        Ok(ChipInitResult::NoError)
    }

    fn get_arch(&self) -> luwen_core::Arch {
        luwen_core::Arch::Blackhole
    }

    fn arc_msg(&self, msg: ArcMsgOptions) -> Result<ArcMsgOk, PlatformError> {
        let code = msg.msg.msg_code();
        let args = msg.msg.args();

        let (_status, rc, response) = self.bh_arc_msg(
            code,
            &[args.0 as u32 | ((args.1 as u32) << 16)],
            Some(msg.timeout),
        )?;
        Ok(ArcMsgOk::Ok {
            rc: rc as u32,
            arg: response[0],
        })
    }

    fn get_neighbouring_chips(&self) -> Result<Vec<NeighbouringChip>, crate::error::PlatformError> {
        Ok(vec![])
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn get_telemetry(&self) -> Result<super::Telemetry, PlatformError> {
        // Get chip telemetry and device data
        // Read telemetry data block address from scratch ram
        // Then read and parse telemetry data

        // Get address of telem struct from scratch ram
        let mut scratch_reg_13_value = [0u8; 4];
        self.axi_read_field(&self.telemetry_struct_addr, &mut scratch_reg_13_value)?;
        let telem_struct_addr = u32::from_le_bytes(scratch_reg_13_value);

        // Read the data block from the address in sctrach 13
        // Parse out the version and entry count before reading the data block
        let mut version: [u8; 4] = [0u8; 4];
        let mut entry_count = [0u8; 4];
        self.axi_read(telem_struct_addr as u64, &mut version)?;
        self.axi_read((telem_struct_addr + 4) as u64, &mut entry_count)?;

        // TODO: Implement version check and data block parsing based on version
        // For now, assume version 1 and parse data block as is
        // let version = u32::from_le_bytes(version);
        let entry_count = u32::from_le_bytes(entry_count);

        // Get telemetry tags data block and telemetry data data block
        let mut telemetry_tags_data_block: Vec<u8> = vec![0u8; entry_count as usize * 4];
        let mut telem_data_block: Vec<u8> = vec![0u8; entry_count as usize * 4];

        self.axi_read(
            (telem_struct_addr + 8) as u64,
            &mut telemetry_tags_data_block,
        )?;
        self.axi_read(
            (telem_struct_addr + 8 + entry_count * 4) as u64,
            &mut telem_data_block,
        )?;

        // Parse telemetry data
        let mut telemetry_data = super::Telemetry::default();
        for i in 0..entry_count as u8 {
            let tag = u32_from_slice(&telemetry_tags_data_block, i) & 0xFF;
            // TODO: Implement offset use
            // let offset = u32_from_slice(&telemetry_tags_data_block, i) >> 16 & 0xFF;
            let data = u32_from_slice(&telem_data_block, i);
            // print!("Tag: {:#02x} Data: {:#02x} Offset {:#02x}\n", tag, data, offset);
            match u32_to_telemetry_tags!(tag) {
                TelemetryTags::ENUM_VERSION => telemetry_data.enum_version = data,
                TelemetryTags::ENTRY_COUNT => telemetry_data.entry_count = data,
                TelemetryTags::BOARD_ID_HIGH => telemetry_data.board_id_high = data,
                TelemetryTags::BOARD_ID_LOW => telemetry_data.board_id_low = data,
                TelemetryTags::ASIC_ID => telemetry_data.asic_id = data,
                TelemetryTags::AICLK => telemetry_data.aiclk = data,
                TelemetryTags::AXICLK => telemetry_data.axiclk = data,
                TelemetryTags::ARCCLK => telemetry_data.arcclk = data,
                TelemetryTags::VCORE => telemetry_data.vcore = data,
                TelemetryTags::TDP => telemetry_data.tdp = data,
                TelemetryTags::TDC => telemetry_data.tdc = data,
                TelemetryTags::VDD_LIMITS => telemetry_data.vdd_limits = data,
                TelemetryTags::THM_LIMITS => telemetry_data.thm_limits = data,
                TelemetryTags::ASIC_TEMPERATURE => telemetry_data.asic_temperature = data,
                TelemetryTags::VREG_TEMPERATURE => telemetry_data.vreg_temperature = data,
                TelemetryTags::BOARD_TEMPERATURE => telemetry_data.board_temperature = data,
                TelemetryTags::L2CPUCLK0 => telemetry_data.l2cpuclk0 = data,
                TelemetryTags::L2CPUCLK1 => telemetry_data.l2cpuclk1 = data,
                TelemetryTags::L2CPUCLK2 => telemetry_data.l2cpuclk2 = data,
                TelemetryTags::L2CPUCLK3 => telemetry_data.l2cpuclk3 = data,
                TelemetryTags::TIMER_HEARTBEAT => telemetry_data.timer_heartbeat = data,
                TelemetryTags::DDR_STATUS => telemetry_data.ddr_status = data,
                TelemetryTags::DDR_SPEED => telemetry_data.ddr_speed = Some(data),
                TelemetryTags::FAN_SPEED => telemetry_data.fan_speed = data,
                _ => (),
            }
        }
        telemetry_data.board_id = 0xb1ac401e;
        Ok(telemetry_data)
    }

    fn get_device_info(&self) -> Result<Option<crate::DeviceInfo>, PlatformError> {
        Ok(self.chip_if.get_device_info()?)
    }
}
