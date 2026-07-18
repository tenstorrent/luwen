// SPDX-FileCopyrightText: © 2024 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use bytemuck::bytes_of;
use num_traits::cast::FromPrimitive;

use std::{mem::size_of, sync::Arc};

use crate::{
    arc_msg::ArcMsgOk,
    chip::{
        communication::{chip_comms::ChipComms, chip_interface::ChipInterface},
        hl_comms::HlCommsInterface,
    },
    error::{BtWrapper, PlatformError},
    ArcMsg, ChipImpl,
};

use super::{
    eth_addr::EthAddr,
    hl_comms::HlComms,
    init::status::{ComponentStatusInfo, InitOptions, WaitStatus},
    remote::EthAddresses,
    ArcMsgOptions, AxiData, ChipInitResult, CommsStatus, InitStatus, NeighbouringChip,
};

pub mod boot_fs;
pub mod ccfgovr;
pub mod message;
pub mod spirom_tables;

#[macro_use]
pub mod telemetry_tags;
use crate::chip::blackhole::telemetry_tags::TelemetryTags;
use prost::Message;
use serde_json::Value;
use std::collections::HashMap;

// pub use telemetry_tags::telemetry_tags_to_u32;

const CSM_START: u32 = 0x10000000;
const CSM_END: u32 = 0x1007FFFF;

fn u32_from_slice(data: &[u8], index: usize) -> Option<u32> {
    let start = index.checked_mul(size_of::<u32>())?;
    let end = start.checked_add(size_of::<u32>())?;
    Some(u32::from_le_bytes(data.get(start..end)?.try_into().ok()?))
}

fn required_telemetry_data_words(tag_data: &[u8], entry_count: usize) -> Option<usize> {
    let required_tag_bytes = entry_count.checked_mul(size_of::<u32>())?;
    if tag_data.len() < required_tag_bytes {
        return None;
    }

    (0..entry_count)
        .map(|index| u32_from_slice(tag_data, index).map(|entry| (entry >> 16) as usize))
        .collect::<Option<Vec<_>>>()?
        .into_iter()
        .max()
        .map_or(Some(0), |max_offset| max_offset.checked_add(1))
}

fn csm_contains_block(address: u32, byte_len: usize) -> bool {
    if !(CSM_START..=CSM_END).contains(&address) {
        return false;
    }
    if byte_len == 0 {
        return true;
    }

    let Ok(byte_len) = u32::try_from(byte_len) else {
        return false;
    };
    address
        .checked_add(byte_len - 1)
        .is_some_and(|end| end <= CSM_END)
}

#[cfg(test)]
mod telemetry_layout_tests {
    use super::{csm_contains_block, required_telemetry_data_words, u32_from_slice, CSM_END};
    use std::mem::size_of;

    fn tag_entry(tag: u16, offset: u16) -> [u8; 4] {
        ((u32::from(offset) << 16) | u32::from(tag)).to_le_bytes()
    }

    #[test]
    fn sparse_offsets_determine_data_span() {
        let tags = [tag_entry(1, 0), tag_entry(2, 17), tag_entry(3, 4)].concat();

        assert_eq!(required_telemetry_data_words(&tags, 3), Some(18));
    }

    #[test]
    fn word_decoder_supports_indices_above_63() {
        let mut words = vec![0u8; 65 * size_of::<u32>()];
        words[64 * size_of::<u32>()..65 * size_of::<u32>()]
            .copy_from_slice(&0xDEADBEEFu32.to_le_bytes());

        assert_eq!(u32_from_slice(&words, 64), Some(0xDEADBEEF));
    }

    #[test]
    fn truncated_tag_table_is_rejected() {
        assert_eq!(required_telemetry_data_words(&tag_entry(1, 0), 2), None);
    }

    #[test]
    fn csm_validation_checks_the_entire_block() {
        assert!(csm_contains_block(CSM_END - 3, 4));
        assert!(!csm_contains_block(CSM_END - 3, 5));
    }
}

#[derive(Clone)]
pub struct Blackhole {
    pub chip_if: Arc<dyn ChipInterface + Send + Sync>,
    pub arc_if: Arc<dyn ChipComms + Send + Sync>,

    pub message_queue: once_cell::sync::OnceCell<message::MessageQueue<8>>,

    pub eth_locations: [EthCore; 14],
    pub eth_addrs: EthAddresses,

    spi_buffer_addr: AxiData,
    telemetry_data_addr: AxiData,
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

#[derive(Debug)]
#[repr(u8)]
pub enum ArcFwInitStatus {
    NotStarted = 0,
    Started = 1,
    Done = 2,
    Error = 3,
    Unknown(u8),
}

impl From<u8> for ArcFwInitStatus {
    fn from(value: u8) -> Self {
        match value {
            0 => ArcFwInitStatus::NotStarted,
            1 => ArcFwInitStatus::Started,
            2 => ArcFwInitStatus::Done,
            3 => ArcFwInitStatus::Error,
            other => ArcFwInitStatus::Unknown(other),
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

        let output = Blackhole {
            chip_if: Arc::new(chip_if),

            message_queue: once_cell::sync::OnceCell::new(),

            eth_addrs: EthAddresses::default(),

            spi_buffer_addr: arc_if.axi_translate("arc_ss.reset_unit.SCRATCH_RAM[10]")?,
            telemetry_data_addr: arc_if.axi_translate("arc_ss.reset_unit.SCRATCH_RAM[12]")?,
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

    pub fn arc_fw_init_status(&self) -> Option<ArcFwInitStatus> {
        self.axi_read32(self.scratch_ram_base.addr + (4 * 2))
            .ok()
            .map(|boot_status_0| ArcFwInitStatus::from(((boot_status_0 >> 1) & 0x3) as u8))
    }

    pub fn check_arc_msg_safe(&self) -> bool {
        // Note that hw_ready can be false while we can safely send an arc_msg
        // This confuses me a bit because this means you can send arc messages that will potentially poke an uninitialized hw
        if let Ok(boot_status_0) = self.axi_read32(self.scratch_ram_base.addr + (4 * 2)) {
            (boot_status_0 & 0x1) == 1
        } else {
            false
        }
    }

    /// Sends a blackhole ARC message.
    ///
    /// # Note
    ///
    /// Data is an (up to) 8-word buffer. Code overwrites the LSB of word 0.
    fn bh_arc_msg(
        &self,
        code: u8,
        data: [u32; 8],
        timeout: Option<std::time::Duration>,
    ) -> Result<(u8, u16, [u32; 8]), PlatformError> {
        if !self.check_arc_msg_safe() {
            return Err(PlatformError::ArcNotReady(
                crate::error::ArcReadyError::BootIncomplete,
                BtWrapper::capture(),
            ));
        }

        // Prepare request from data buffer
        let mut request = [0; 8];
        for (src, dst) in data.iter().copied().zip(request.iter_mut()) {
            *dst = src;
        }
        // Override message ID
        //
        // This is the LSB of the first word.
        request[0] &= !0xff;
        request[0] |= u32::from(code);

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
                response[0],
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

        /*
         * Since SPI unlock/lock commands are not supported in older firmwares,
         * we will ignore errors from them and return a default status
         */
        let (status, _, _) = self
            .bh_arc_msg(0xC2, [0, 0, 0, 0, 0, 0, 0, 0], None)
            .unwrap_or_default();

        if status != 0 {
            return Err("Failed to unlock spi".into());
        }

        for chunk in value.chunks(buffer.size as usize) {
            self.axi_write(buffer.addr as u64, chunk)?;
            let (status, _, _) = self.bh_arc_msg(
                0x1A,
                [0, addr, chunk.len() as u32, buffer.addr, 0, 0, 0, 0],
                None,
            )?;

            std::thread::sleep(std::time::Duration::from_millis(100));

            if status != 0 {
                return Err("Failed to write to SPI".into());
            }

            addr += chunk.len() as u32;
        }

        /* Similar to above, ignore errors from lock command */
        let (status, _, _) = self
            .bh_arc_msg(0xC3, [0, 0, 0, 0, 0, 0, 0, 0], None)
            .unwrap_or_default();

        if status != 0 {
            return Err("Failed to lock spi".into());
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
                [0, addr, chunk.len() as u32, buffer.addr, 0, 0, 0, 0],
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
        let spi_reader = |addr: u32, size: usize| {
            let mut buf = vec![0; size];
            self.spi_read(addr, &mut buf).unwrap();
            buf
        };
        Ok(boot_fs::read_tag(spi_reader, tag_name))
    }

    pub fn decode_boot_fs_table(
        &self,
        tag_name: &str,
    ) -> Result<HashMap<String, Value>, Box<dyn std::error::Error>> {
        // Return the decoded boot fs table as a HashMap
        // Get the spi address and image size of the tag and read the proto bin
        // Decode the proto bin and convert it to a HashMap
        let tag_info = self
            .get_boot_fs_tables_spi_read(tag_name)?
            .ok_or_else(|| format!("Tag '{tag_name}' not found in boot FS tables"))?;
        let spi_addr = tag_info.1.spi_addr;
        let image_size = tag_info.1.flags.image_size();

        // declare as vec to allow non-const size
        let mut proto_bin = vec![0u8; image_size as usize];
        self.spi_read(spi_addr, &mut proto_bin)?;
        let final_decode_map: HashMap<String, Value>;
        // remove padding
        proto_bin = spirom_tables::remove_padding_proto_bin(&proto_bin)?.to_vec();

        if tag_name == "cmfwcfg" || tag_name == "origcfg" {
            final_decode_map =
                spirom_tables::to_hash_map(spirom_tables::fw_table::FwTable::decode(&*proto_bin)?);
        } else if tag_name == "boardcfg" {
            final_decode_map = spirom_tables::to_hash_map(
                spirom_tables::read_only::ReadOnly::decode(&*proto_bin)?,
            );
        } else if tag_name == "flshinfo" {
            final_decode_map = spirom_tables::to_hash_map(
                spirom_tables::flash_info::FlashInfoTable::decode(&*proto_bin)?,
            );
        } else {
            return Err(format!("Unsupported tag name: {tag_name}").into());
        };
        Ok(final_decode_map)
    }

    pub fn encode_and_write_boot_fs_table(
        &self,
        hashmap: HashMap<String, Value>,
        tag_name: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Convert the HashMap to a proto message and encode it to a proto bin
        let mut proto_bin = if tag_name == "cmfwcfg" {
            spirom_tables::from_hash_map::<spirom_tables::fw_table::FwTable>(hashmap)
                .encode_to_vec()
        } else if tag_name == "boardcfg" {
            spirom_tables::from_hash_map::<spirom_tables::read_only::ReadOnly>(hashmap)
                .encode_to_vec()
        } else if tag_name == "flshinfo" {
            spirom_tables::from_hash_map::<spirom_tables::flash_info::FlashInfoTable>(hashmap)
                .encode_to_vec()
        } else {
            return Err(format!("Unsupported tag name: {tag_name}").into());
        };
        // Pad the proto bin to be a multiple of 4 bytes to fit into the spirom requirements
        let padding = 4 - (proto_bin.len() % 4);
        for i in 0..padding {
            proto_bin.push(i as u8);
        }

        // Write the proto bin to the spirom and update the checksums
        let tag_info = self
            .get_boot_fs_tables_spi_read(tag_name)?
            .ok_or_else(|| format!("Tag '{tag_name}' not found in boot FS tables"))?;

        let mut fd_in_spi = tag_info.1;
        fd_in_spi.flags.set_image_size(proto_bin.len() as u32);

        let data_chk = spirom_tables::calculate_checksum(&proto_bin);
        fd_in_spi.data_crc = data_chk;
        fd_in_spi.fd_crc = 0;

        // do length -4 because of a bug in checksum calculation in bootrom
        let fd_chk = {
            let fd_bytes = bytes_of(&fd_in_spi);
            spirom_tables::calculate_checksum(&fd_bytes[..fd_bytes.len() - 4])
        };
        fd_in_spi.fd_crc = fd_chk;

        self.spi_write(tag_info.0, bytes_of(&fd_in_spi))?;
        self.spi_write(fd_in_spi.spi_addr, &proto_bin)?;

        Ok(())
    }

    /// Read the active ccfgovr override map (empty if neither bank validates).
    pub fn ccfgovr_read(&self) -> Result<HashMap<String, Value>, Box<dyn std::error::Error>> {
        ccfgovr::read_active(self)
    }

    /// Read the merged effective view: cmfwcfg with the active override applied.
    pub fn ccfgovr_effective(&self) -> Result<HashMap<String, Value>, Box<dyn std::error::Error>> {
        ccfgovr::read_effective(self)
    }

    /// Read both banks' raw state.
    pub fn ccfgovr_read_state(&self) -> Result<ccfgovr::State, Box<dyn std::error::Error>> {
        ccfgovr::State::read(self)
    }

    /// Write `map` as the next ccfgovr override to the inactive bank.
    /// Returns the bank that is now active (the one just written).
    pub fn ccfgovr_write(
        &self,
        map: HashMap<String, Value>,
    ) -> Result<ccfgovr::Bank, Box<dyn std::error::Error>> {
        ccfgovr::write(self, map)
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
                        let msg_safe = self.check_arc_msg_safe();
                        let fw_status = self.arc_fw_init_status();

                        if let Some(fw_status) = fw_status {
                            match fw_status {
                                ArcFwInitStatus::NotStarted => {
                                    *status_string = Some("BH FW boot not started".to_string());
                                }
                                ArcFwInitStatus::Started => {
                                    *status_string = Some("BH FW boot not complete".to_string());
                                }
                                ArcFwInitStatus::Done => {
                                    if !msg_safe {
                                        *status_string = Some(
                                            "BH FW arc msg queue init not complete".to_string(),
                                        );
                                    } else {
                                        *s = WaitStatus::JustFinished;
                                    }
                                }
                                ArcFwInitStatus::Error => {
                                    *status_string = Some("BH FW Boot error".to_string());
                                    *s = WaitStatus::Error(
                                        super::init::status::ArcInitError::WaitingForInit(
                                            crate::error::ArcReadyError::BootError,
                                        ),
                                    );
                                }
                                ArcFwInitStatus::Unknown(status) => {
                                    *status_string = Some(format!("BH FW Boot status unknown {status} (will wait to see if it becomes known)"));
                                }
                            }

                            // If we are still waiting after changing status and (potentially) reassigning to a hard error
                            if let WaitStatus::Waiting(_) = s {
                                // and we timed out, then raise a boot incomplete error
                                if status.start_time.elapsed() > status.timeout {
                                    *s = WaitStatus::Error(
                                        super::init::status::ArcInitError::WaitingForInit(
                                            crate::error::ArcReadyError::BootIncomplete,
                                        ),
                                    );
                                }
                            }
                        } else {
                            *status_string =
                                Some("Failed to access fw to read init status".to_string());
                            *s = WaitStatus::Error(
                                super::init::status::ArcInitError::WaitingForInit(
                                    crate::error::ArcReadyError::NoAccess,
                                ),
                            );
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

    fn get_arch(&self) -> luwen_def::Arch {
        luwen_def::Arch::Blackhole
    }

    fn arc_msg(&self, msg: ArcMsgOptions) -> Result<ArcMsgOk, PlatformError> {
        let code = msg.msg.msg_code();
        let data = if let ArcMsg::Buf(msg) = msg.msg {
            msg
        } else {
            let args = msg.msg.args();
            [0, args.0 as u32 | ((args.1 as u32) << 16), 0, 0, 0, 0, 0, 0]
        };

        let (_status, rc, response) = self.bh_arc_msg(code as u8, data, Some(msg.timeout))?;
        Ok(match msg.msg {
            ArcMsg::Buf(_) => ArcMsgOk::OkBuf(response),
            _ => ArcMsgOk::Ok {
                rc: rc as u32,
                arg: response[1],
            },
        })
    }

    fn get_neighbouring_chips(&self) -> Result<Vec<NeighbouringChip>, crate::error::PlatformError> {
        Ok(vec![])
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn get_telemetry(&self) -> Result<super::Telemetry, PlatformError> {
        let mut scratch_reg_13_value = [0u8; 4];
        self.axi_read_field(&self.telemetry_struct_addr, &mut scratch_reg_13_value)?;
        let telem_struct_addr = u32::from_le_bytes(scratch_reg_13_value);
        if telem_struct_addr == 0 {
            return Err(PlatformError::ArcNotReady(
                crate::error::ArcReadyError::BootIncomplete,
                BtWrapper::capture(),
            ));
        }
        if !csm_contains_block(telem_struct_addr, 8) {
            return Err(PlatformError::Generic(
                format!("Invalid Telemetry struct address: 0x{telem_struct_addr:08x}"),
                BtWrapper::capture(),
            ));
        }

        let mut scratch_reg_12_value = [0u8; 4];
        self.axi_read_field(&self.telemetry_data_addr, &mut scratch_reg_12_value)?;
        let telem_data_addr = u32::from_le_bytes(scratch_reg_12_value);
        if telem_data_addr == 0 {
            return Err(PlatformError::ArcNotReady(
                crate::error::ArcReadyError::BootIncomplete,
                BtWrapper::capture(),
            ));
        }

        let _version = self.axi_read32(telem_struct_addr as u64)?;
        let entry_count = self.axi_read32(telem_struct_addr as u64 + 4)? as usize;

        // TODO: Implement version check and data block parsing based on version
        // For now, assume version 1 and parse data block as is
        // let version = u32::from_le_bytes(version);

        let tag_bytes = entry_count.checked_mul(size_of::<u32>()).ok_or_else(|| {
            PlatformError::Generic(
                format!("Invalid Telemetry entry count: {entry_count}"),
                BtWrapper::capture(),
            )
        })?;
        let tag_data_addr = telem_struct_addr.checked_add(8).ok_or_else(|| {
            PlatformError::Generic(
                format!("Invalid Telemetry struct address: 0x{telem_struct_addr:08x}"),
                BtWrapper::capture(),
            )
        })?;
        if !csm_contains_block(tag_data_addr, tag_bytes) {
            return Err(PlatformError::Generic(
                format!(
                    "Telemetry tag table at 0x{tag_data_addr:08x} with {entry_count} entries exceeds CSM"
                ),
                BtWrapper::capture(),
            ));
        }

        let mut telemetry_tags_data_block = vec![0u8; tag_bytes];
        self.axi_read(tag_data_addr as u64, &mut telemetry_tags_data_block)?;

        let data_words = required_telemetry_data_words(&telemetry_tags_data_block, entry_count)
            .ok_or_else(|| {
                PlatformError::Generic(
                    "Invalid Telemetry tag table".to_string(),
                    BtWrapper::capture(),
                )
            })?;
        let data_bytes = data_words.checked_mul(size_of::<u32>()).ok_or_else(|| {
            PlatformError::Generic(
                format!("Invalid Telemetry data table size: {data_words} words"),
                BtWrapper::capture(),
            )
        })?;
        if !csm_contains_block(telem_data_addr, data_bytes) {
            return Err(PlatformError::Generic(
                format!(
                    "Telemetry data table at 0x{telem_data_addr:08x} with {data_words} words exceeds CSM"
                ),
                BtWrapper::capture(),
            ));
        }

        let mut telem_data_block = vec![0u8; data_bytes];
        if data_bytes != 0 {
            self.axi_read(telem_data_addr as u64, &mut telem_data_block)?;
        }

        // Parse telemetry data
        let mut telemetry_data = super::Telemetry::default();
        for i in 0..entry_count {
            let entry = u32_from_slice(&telemetry_tags_data_block, i).ok_or_else(|| {
                PlatformError::Generic(
                    format!("Missing Telemetry tag table entry {i}"),
                    BtWrapper::capture(),
                )
            })?;
            let tag = entry & 0xFFFF;
            let offset = (entry >> 16) & 0xFFFF;
            let data = u32_from_slice(&telem_data_block, offset as usize).ok_or_else(|| {
                PlatformError::Generic(
                    format!("Invalid Telemetry data offset: {offset}"),
                    BtWrapper::capture(),
                )
            })?;
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
                    TelemetryTags::TimerHeartbeat => {
                        telemetry_data.timer_heartbeat = data;
                    }
                    TelemetryTags::TelemEnumCount => telemetry_data.entry_count = data,
                    TelemetryTags::EnabledTensixCol => telemetry_data.tensix_enabled_col = data,
                    TelemetryTags::EnabledEth => telemetry_data.enabled_eth = data,
                    TelemetryTags::EnabledGddr => telemetry_data.enabled_gddr = data,
                    TelemetryTags::EnabledL2Cpu => telemetry_data.enabled_l2cpu = data,
                    TelemetryTags::PcieUsage => telemetry_data.enabled_pcie = data,
                    TelemetryTags::NocTranslation => {
                        telemetry_data.noc_translation_enabled = data != 0
                    }
                    TelemetryTags::FanRpm => telemetry_data.fan_rpm = data,
                    TelemetryTags::Gddr01Temp => telemetry_data.gddr01_temp = data,
                    TelemetryTags::Gddr23Temp => telemetry_data.gddr23_temp = data,
                    TelemetryTags::Gddr45Temp => telemetry_data.gddr45_temp = data,
                    TelemetryTags::Gddr67Temp => telemetry_data.gddr67_temp = data,
                    TelemetryTags::Gddr01CorrErrs => telemetry_data.gddr01_corr_errs = data,
                    TelemetryTags::Gddr23CorrErrs => telemetry_data.gddr23_corr_errs = data,
                    TelemetryTags::Gddr45CorrErrs => telemetry_data.gddr45_corr_errs = data,
                    TelemetryTags::Gddr67CorrErrs => telemetry_data.gddr67_corr_errs = data,
                    TelemetryTags::GddrUncorrErrs => telemetry_data.gddr_uncorr_errs = data,
                    TelemetryTags::MaxGddrTemp => telemetry_data.max_gddr_temp = data,
                    TelemetryTags::AsicLocation => telemetry_data.asic_location = data,
                    TelemetryTags::BoardPowerLimit => telemetry_data.board_power_limit = data,
                    TelemetryTags::InputPower => telemetry_data.input_power = data,
                    TelemetryTags::TdcLimitMax => telemetry_data.tdc_limit_max = data,
                    TelemetryTags::ThmLimitThrottle => telemetry_data.thm_limit_throttle = data,
                    TelemetryTags::ThermTripCount => telemetry_data.therm_trip_count = data,
                    TelemetryTags::AsicIdHigh => telemetry_data.asic_id_high = data,
                    TelemetryTags::AsicIdLow => telemetry_data.asic_id_low = data,
                    TelemetryTags::AiclkLimitMax => telemetry_data.aiclk_limit_max = data,
                    TelemetryTags::TdpLimitMax => telemetry_data.tdp_limit_max = data,
                    _ => (),
                }
            }
        }
        telemetry_data.board_id =
            ((telemetry_data.board_id_high as u64) << 32) | telemetry_data.board_id_low as u64;
        telemetry_data.arch = self.get_arch();
        Ok(telemetry_data)
    }

    fn get_device_info(&self) -> Result<Option<crate::DeviceInfo>, PlatformError> {
        Ok(self.chip_if.get_device_info()?)
    }
}
