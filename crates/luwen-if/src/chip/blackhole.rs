// SPDX-FileCopyrightText: © 2024 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use num_traits::cast::FromPrimitive;

use std::sync::Arc;

use crate::{
    arc_msg::ArcMsgOk,
    chip::{
        communication::{chip_comms::ChipComms, chip_interface::ChipInterface},
        hl_comms::HlCommsInterface,
    },
    error::{BtWrapper, PlatformError},
    ChipImpl,
};

use super::{
    eth_addr::EthAddr,
    hl_comms::HlComms,
    init::status::{ComponentStatusInfo, InitOptions, WaitStatus},
    remote::EthAddresses,
    ArcMsgOptions, AxiData, ChipInitResult, CommsStatus, InitStatus, NeighbouringChip,
};

pub mod boot_fs;
pub mod message;
pub mod spirom_tables;

#[macro_use]
pub mod telemetry_tags;
use crate::chip::blackhole::telemetry_tags::TelemetryTags;
use prost::Message;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

// pub use telemetry_tags::telemetry_tags_to_u32;

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

    pub message_queue: once_cell::sync::OnceCell<message::MessageQueue<8>>,

    pub eth_locations: [EthCore; 14],
    pub eth_addrs: EthAddresses,

    spi_buffer_addr: AxiData,
    telemetry_struct_addr: AxiData,
    scratch_ram_base: AxiData,
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

struct SpiBuffer {
    addr: u32,
    size: u32,
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

        let output = Blackhole {
            chip_if: Arc::new(chip_if),

            message_queue: once_cell::sync::OnceCell::new(),

            eth_addrs: EthAddresses::default(),

            spi_buffer_addr: arc_if.axi_translate("arc_ss.reset_unit.SCRATCH_RAM[10]")?,
            telemetry_struct_addr: arc_if.axi_translate("arc_ss.reset_unit.SCRATCH_RAM[13]")?,
            scratch_ram_base: arc_if.axi_translate("arc_ss.reset_unit.SCRATCH_RAM[0]")?,

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

    pub fn hw_ready(&self) -> bool {
        if let Ok(boot_status_0) = self.axi_read32(self.scratch_ram_base.addr + (4 * 2)) {
            ((boot_status_0 >> 1) & 0x3) == 2
        } else {
            false
        }
    }

    pub fn check_arc_msg_safe(&self) -> bool {
        if !self.hw_ready() {
            return false;
        }

        if let Ok(boot_status_0) = self.axi_read32(self.scratch_ram_base.addr + (4 * 2)) {
            (boot_status_0 & 0x1) == 1
        } else {
            false
        }
    }

    fn bh_arc_msg(
        &self,
        code: u8,
        zero_data: Option<u32>,
        data: &[u32],
        timeout: Option<std::time::Duration>,
    ) -> Result<(u8, u16, [u32; 7]), PlatformError> {
        if !self.check_arc_msg_safe() {
            return Err(PlatformError::ArcNotReady(
                crate::error::ArcReadyError::BootIncomplete,
                BtWrapper::capture(),
            ));
        }

        let mut request = [0; 8];
        request[0] = code as u32 | zero_data.unwrap_or(0);
        for (i, o) in data.iter().zip(request[1..].iter_mut()) {
            *o = *i;
        }

        let timeout = timeout.unwrap_or(std::time::Duration::from_millis(500));

        let queue = self.message_queue.get_or_try_init::<_, PlatformError>(|| {
            let message_queue_info_address = self
                .arc_if
                .axi_sread32(&self.chip_if, "arc_ss.reset_unit.SCRATCH_RAM[11]")?
                as u64;
            let queue_base = self
                .arc_if
                .axi_read32(&self.chip_if, message_queue_info_address)?;
            let queue_sizing = self
                .arc_if
                .axi_read32(&self.chip_if, message_queue_info_address + 4)?;
            let queue_size = queue_sizing & 0xFF;
            let queue_count = (queue_sizing >> 8) & 0xFF;

            Ok(message::MessageQueue {
                header_size: 8,
                entry_size: 8,
                queue_base: queue_base as u64,
                queue_size,
                queue_count,
                fw_int: self
                    .arc_if
                    .axi_translate("arc_ss.reset_unit.ARC_MISC_CNTL.irq0_trig")?,
            })
        })?;

        let response = queue.send_message(self, 2, request, timeout)?;
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
                    source: crate::ArcMsgProtocolError::MsgNotRecognized(code as u16),
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

    fn get_spi_buffer(&self) -> Result<SpiBuffer, Box<dyn std::error::Error>> {
        let buffer_addr = self.axi_read32(self.spi_buffer_addr.addr)?;

        Ok(SpiBuffer {
            addr: (buffer_addr & 0xFFFFFF) + 0x10000000, /* magic offset to translate to the correct buffer address */
            size: 1 << ((buffer_addr >> 24) & 0xFF),
        })
    }

    pub fn spi_write(&self, mut addr: u32, value: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        let buffer = self.get_spi_buffer()?;

        for chunk in value.chunks(buffer.size as usize) {
            self.axi_write(buffer.addr as u64, chunk)?;
            let (status, _, _) = self.bh_arc_msg(
                0x1A,
                Some(0 << 8),
                &[addr, chunk.len() as u32, buffer.addr],
                None,
            )?;

            std::thread::sleep(std::time::Duration::from_millis(100));

            if status != 0 {
                return Err("Failed to write to SPI".into());
            }

            addr += chunk.len() as u32;
        }

        Ok(())
    }

    pub fn spi_read(
        &self,
        mut addr: u32,
        value: &mut [u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let buffer = self.get_spi_buffer()?;

        for chunk in value.chunks_mut(buffer.size as usize) {
            let (status, _, _) = self.bh_arc_msg(
                0x19,
                Some(0 << 8),
                &[addr, chunk.len() as u32, buffer.addr],
                None,
            )?;

            if status != 0 {
                return Err("Failed to read from SPI".into());
            }

            self.axi_read(buffer.addr as u64, chunk)?;

            addr += chunk.len() as u32;
        }

        Ok(())
    }

    pub fn get_local_chip_coord(&self) -> Result<EthAddr, PlatformError> {
        Ok(EthAddr {
            rack_x: 0,
            rack_y: 0,
            shelf_x: 0,
            shelf_y: 0,
        })
    }

    pub fn get_boot_fs_tables_spi_read(
        &self,
        tag_name: &str,
    ) -> Result<Option<(u32, boot_fs::TtBootFsFd)>, Box<dyn std::error::Error>> {
        let reader = |addr: u32, size: usize| {
            let mut buf = vec![0; size];
            self.spi_read(addr, &mut buf).unwrap();
            buf
        };
        Ok(boot_fs::read_tag(
            &reader as &dyn Fn(u32, usize) -> Vec<u8>,
            tag_name,
        ))
    }

    fn remove_padding_proto_bin<'a>(
        &self,
        bincode: &'a [u8],
    ) -> Result<&'a [u8], Box<dyn std::error::Error>> {
        // The proto bins have to be padded to be a multiple of 4 bytes to fit into the spirom requirements
        // This means that we have to read the last byte of the bin and remove num + 1 num of bytes
        // 0: remove 1 byte (0)
        // 1: remove 2 bytes (0, 1)
        // 2: remove 3 bytes (0, X, 2)
        // 3: remove 4 bytes (0, X, X, 3)

        let last_byte = bincode[bincode.len() - 1] as usize;
        // truncate the last byte and the padding bytes
        Ok(&bincode[..bincode.len() - last_byte - 1])
    }

    /// Generic function to convert any serializable type into a HashMap
    fn to_hash_map<T: Serialize>(&self, value: T) -> HashMap<String, Value> {
        // Serialize the value to JSON
        let json_string = serde_json::to_string(&value).unwrap();
        // Deserialize the JSON into a HashMap
        serde_json::from_str(&json_string).unwrap()
    }

    pub fn decode_boot_fs_table(
        &self,
        tag_name: &str,
    ) -> Result<HashMap<String, Value>, Box<dyn std::error::Error>> {
        // Return the decoded boot fs table as a HashMap
        // Get the spi address and image size of the tag and read the proto bin
        // Decode the proto bin and convert it to a HashMap
        let table = self
            .get_boot_fs_tables_spi_read(tag_name)?
            .ok_or_else(|| format!("Non-existent tag name: {}", tag_name))?
            .1;
        let spi_addr = table.spi_addr;
        let image_size = unsafe { table.flags.f.image_size() };
        // declare as vec to allow non-const size
        let mut proto_bin = vec![0u8; image_size as usize];
        self.spi_read(spi_addr, &mut proto_bin)?;
        let final_decode_map: HashMap<String, Value>;
        // remove padding
        proto_bin = self.remove_padding_proto_bin(&proto_bin)?.to_vec();

        if tag_name == "cmfwcfg" || tag_name == "origcfg" {
            final_decode_map =
                self.to_hash_map(spirom_tables::fw_table::FwTable::decode(&*proto_bin)?);
        } else if tag_name == "boardcfg" {
            final_decode_map =
                self.to_hash_map(spirom_tables::read_only::ReadOnly::decode(&*proto_bin)?);
        } else if tag_name == "flshinfo" {
            final_decode_map = self.to_hash_map(spirom_tables::flash_info::FlashInfoTable::decode(
                &*proto_bin,
            )?);
        } else {
            return Err(format!("Unsupported tag name: {}", tag_name).into());
        };
        Ok(final_decode_map)
    }
}

fn default_status() -> InitStatus {
    InitStatus {
        comms_status: super::CommsStatus::CanCommunicate,
        arc_status: ComponentStatusInfo {
            name: "ARC".to_string(),
            wait_status: Box::new([WaitStatus::Waiting(None)]),

            start_time: std::time::Instant::now(),
            timeout: std::time::Duration::from_secs(5),
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
                match s {
                    WaitStatus::Waiting(status_string) => {
                        if self.check_arc_msg_safe() {
                            *s = WaitStatus::JustFinished;
                        } else {
                            *status_string = Some("BH FW boot not complete".to_string());
                            if status.start_time.elapsed() > status.timeout {
                                *s = WaitStatus::Error(
                                    super::init::status::ArcInitError::WaitingForInit(
                                        crate::error::ArcReadyError::BootIncomplete,
                                    ),
                                );
                            }
                        }
                    }
                    WaitStatus::JustFinished => {
                        *s = WaitStatus::Done;
                    }
                    _ => {}
                }
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
            code as u8,
            None,
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
        // Check if the address is within CSM memory. Otherwise, it must be invalid
        if !(0x10000000..=0x1007FFFF).contains(&telem_struct_addr) {
            return Err(PlatformError::Generic(
                format!(
                    "Invalid Telemetry struct address: 0x{:08x}",
                    telem_struct_addr
                ),
                BtWrapper::capture(),
            ));
        }

        if telem_struct_addr == 0 {
            return Err(PlatformError::ArcNotReady(
                crate::error::ArcReadyError::BootIncomplete,
                BtWrapper::capture(),
            ));
        }

        // Read the data block from the address in sctrach 13
        // Parse out the version and entry count before reading the data block
        let _version = self.axi_read32(telem_struct_addr as u64)?;
        let entry_count = self.axi_read32(telem_struct_addr as u64 + 4)?;

        // TODO: Implement version check and data block parsing based on version
        // For now, assume version 1 and parse data block as is
        // let version = u32::from_le_bytes(version);

        // Get telemetry tags data block and telemetry data data block
        let mut telemetry_tags_data_block: Vec<u8> = vec![0u8; (entry_count + 1) as usize * 4];
        let mut telem_data_block: Vec<u8> = vec![0u8; (entry_count + 1) as usize * 4];

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
            let entry = u32_from_slice(&telemetry_tags_data_block, i);
            let tag = entry & 0xFF;
            let offset = (entry >> 16) & 0xFF;
            let data = u32_from_slice(&telem_data_block, offset as u8);
            // print!("Tag: {} Data: {:#02x}\n", tag, data);
            if let Some(tag) = TelemetryTags::from_u32(tag) {
                match tag {
                    TelemetryTags::BoardIdHigh => telemetry_data.board_id_high = data,
                    TelemetryTags::BoardIdLow => telemetry_data.board_id_low = data,
                    TelemetryTags::AsicId => telemetry_data.asic_id = data,
                    TelemetryTags::HarvestingState => telemetry_data.harvesting_state = data,
                    TelemetryTags::UpdateTelemSpeed => telemetry_data.update_telem_speed = data,
                    TelemetryTags::Vcore => telemetry_data.vcore = data,
                    TelemetryTags::Tdp => telemetry_data.tdp = data,
                    TelemetryTags::Tdc => telemetry_data.tdc = data,
                    TelemetryTags::VddLimits => telemetry_data.vdd_limits = data,
                    TelemetryTags::ThmLimits => telemetry_data.thm_limits = data,
                    TelemetryTags::AsicTemperature => telemetry_data.asic_temperature = data,
                    TelemetryTags::VregTemperature => telemetry_data.vreg_temperature = data,
                    TelemetryTags::BoardTemperature => telemetry_data.board_temperature = data,
                    TelemetryTags::AiClk => telemetry_data.aiclk = data,
                    TelemetryTags::AxiClk => telemetry_data.axiclk = data,
                    TelemetryTags::ArcClk => telemetry_data.arcclk = data,
                    TelemetryTags::L2CPUClk0 => telemetry_data.l2cpuclk0 = data,
                    TelemetryTags::L2CPUClk1 => telemetry_data.l2cpuclk1 = data,
                    TelemetryTags::L2CPUClk2 => telemetry_data.l2cpuclk2 = data,
                    TelemetryTags::L2CPUClk3 => telemetry_data.l2cpuclk3 = data,
                    TelemetryTags::EthLiveStatus => telemetry_data.eth_status0 = data,
                    TelemetryTags::DdrStatus => telemetry_data.ddr_status = data,
                    TelemetryTags::DdrSpeed => telemetry_data.ddr_speed = Some(data),
                    TelemetryTags::EthFwVersion => telemetry_data.eth_fw_version = data,
                    TelemetryTags::DdrFwVersion => telemetry_data.ddr_fw_version = data,
                    TelemetryTags::BmAppFwVersion => telemetry_data.m3_app_fw_version = data,
                    TelemetryTags::BmBlFwVersion => telemetry_data.m3_bl_fw_version = data,
                    TelemetryTags::FlashBundleVersion => telemetry_data.fw_bundle_version = data,
                    // TelemetryTags::CM_FW_VERSION => telemetry_data.cm_fw_version = data,
                    TelemetryTags::L2cpuFwVersion => telemetry_data.l2cpu_fw_version = data,
                    TelemetryTags::FanSpeed => telemetry_data.fan_speed = data,
                    TelemetryTags::TimerHeartbeat => telemetry_data.timer_heartbeat = data,
                    TelemetryTags::TelemEnumCount => telemetry_data.entry_count = data,
                    _ => (),
                }
            }
        }
        telemetry_data.board_id =
            (telemetry_data.board_id_high as u64) << 32 | telemetry_data.board_id_low as u64;
        Ok(telemetry_data)
    }

    fn get_device_info(&self) -> Result<Option<crate::DeviceInfo>, PlatformError> {
        Ok(self.chip_if.get_device_info()?)
    }
}
