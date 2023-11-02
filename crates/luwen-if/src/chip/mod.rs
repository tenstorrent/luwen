// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

mod spi;
mod communication;
mod creation;
pub mod eth_addr;
mod grayskull;
mod hl_comms;
mod init;
mod remote;
mod wormhole;

pub use communication::chip_comms::{
    axi_translate, ArcIf, AxiData, AxiError, ChipComms, MemorySlice, MemorySlices,
};
pub use communication::chip_interface::ChipInterface;
pub use grayskull::Grayskull;
pub use hl_comms::{HlComms, HlCommsInterface};
pub use init::{wait_for_init, CallReason, ChipDetectState};
use luwen_core::Arch;
pub use wormhole::Wormhole;

use crate::{arc_msg::ArcMsgAddr, error::PlatformError, DeviceInfo};

pub use crate::arc_msg::{ArcMsg, ArcMsgOk};

/// Arc message interface
#[derive(Debug)]
pub struct ArcMsgOptions {
    pub msg: ArcMsg,
    pub wait_for_done: bool,
    pub timeout: std::time::Duration,
    pub use_second_mailbox: bool,
    pub addrs: Option<ArcMsgAddr>,
}

impl Default for ArcMsgOptions {
    fn default() -> Self {
        Self {
            msg: ArcMsg::Nop,
            wait_for_done: true,
            timeout: std::time::Duration::from_secs(1),
            use_second_mailbox: false,
            addrs: None,
        }
    }
}

#[derive(Debug, Hash, PartialEq, Eq)]
pub struct NeighbouringChip {
    pub local_noc_addr: (u8, u8),
    pub remote_noc_addr: (u8, u8),
    pub eth_addr: crate::EthAddr,
}

pub enum ArcStatus {
    ArcError(String),
    DramTraining,
    ArcOk,
}

#[derive(Debug, Clone)]
pub enum WaitStatus {
    Waiting {
        start: std::time::Instant,
        timeout: std::time::Duration,
    },
    Timeout(std::time::Duration),
    JustFinished,
    Done,
    NotPresent,
}

impl WaitStatus {
    pub fn is_done(&self) -> bool {
        match self {
            Self::Done => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct StatusInfo {
    pub ready: usize,
    pub total: usize,
    pub wait_status: WaitStatus,
    pub status: String,
}

impl StatusInfo {
    pub fn not_present() -> Self {
        Self {
            wait_status: WaitStatus::NotPresent,
            ready: 0,
            total: 0,
            status: String::new(),
        }
    }

    pub fn get_status(&self) -> String {
        match &self.wait_status {
            WaitStatus::Waiting { .. } => {
                let status_line = if !self.status.is_empty() {
                    format!(": {}", self.status)
                } else {
                    format!("")
                };

                let total_line = if self.total > 0 {
                    format!("[{}/{}]", self.ready, self.total)
                } else {
                    format!("")
                };

                format!("{total_line}{status_line}")
            }
            WaitStatus::Timeout(timeout) => {
                let status_line = if !self.status.is_empty() {
                    format!(": {}", self.status)
                } else {
                    format!("")
                };

                let timeout_line = format!("Timeout after {}", timeout.as_secs());

                format!("{timeout_line}{status_line}")
            }
            WaitStatus::JustFinished | WaitStatus::Done => {
                let status_line = if !self.status.is_empty() {
                    format!(": {}", self.status)
                } else {
                    format!("")
                };

                if self.is_error() {
                    format!("Error{status_line}")
                } else {
                    format!("Completed{status_line}")
                }
            }
            WaitStatus::NotPresent => String::from("No Present"),
        }
    }

    pub fn is_waiting(&self) -> bool {
        match &self.wait_status {
            WaitStatus::Waiting { .. } => true,
            _ => false,
        }
    }

    pub fn is_error(&self) -> bool {
        match &self.wait_status {
            WaitStatus::Timeout(_) => true,
            WaitStatus::JustFinished | WaitStatus::Done => self.ready != self.total,
            _ => false,
        }
    }

    pub fn is_completed(&self) -> bool {
        match &self.wait_status {
            WaitStatus::Timeout(_) | WaitStatus::Done | WaitStatus::JustFinished => true,
            // Not present shouldn't hold up the complete check
            WaitStatus::NotPresent => true,
            _ => false,
        }
    }

    pub fn just_finished(&self) -> bool {
        match &self.wait_status {
            WaitStatus::JustFinished => true,
            _ => false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct InitOptions {
    /// If false, then we will not try to initialize anything that would require talking on the NOC
    noc_safe: bool,
}

#[derive(Clone, Debug)]
pub struct InitStatus {
    pub arc_status: StatusInfo,
    pub dram_status: StatusInfo,
    pub eth_status: StatusInfo,
    pub cpu_status: StatusInfo,

    pub init_options: InitOptions,
}

impl InitStatus {
    pub fn is_waiting(&self) -> bool {
        self.arc_status.is_waiting()
            && self.dram_status.is_waiting()
            && self.eth_status.is_waiting()
            && self.cpu_status.is_waiting()
    }

    pub fn init_complete(&self) -> bool {
        self.arc_status.is_completed()
            && self.dram_status.is_completed()
            && self.eth_status.is_completed()
            && self.cpu_status.is_completed()
    }

    pub fn init_error(&self) -> bool {
        self.arc_status.is_error()
            || self.dram_status.is_error()
            || self.eth_status.is_error()
            || self.cpu_status.is_error()
    }
}

pub enum InitType {
    Arc,
    Dram,
    Eth,
    Cpu,
}

#[derive(Default, Debug)]
pub struct Telemetry {
    pub board_id: u64,
    pub smbus_tx_enum_version: u32,
    pub smbus_tx_device_id: u32,
    pub smbus_tx_asic_ro: u32,
    pub smbus_tx_asic_idd: u32,
    pub smbus_tx_board_id_high: u32,
    pub smbus_tx_board_id_low: u32,
    pub smbus_tx_arc0_fw_version: u32,
    pub smbus_tx_arc1_fw_version: u32,
    pub smbus_tx_arc2_fw_version: u32,
    pub smbus_tx_arc3_fw_version: u32,
    pub smbus_tx_spibootrom_fw_version: u32,
    pub smbus_tx_eth_fw_version: u32,
    pub smbus_tx_m3_bl_fw_version: u32,
    pub smbus_tx_m3_app_fw_version: u32,
    pub smbus_tx_ddr_speed: Option<u32>,
    pub smbus_tx_ddr_status: u32,
    pub smbus_tx_eth_status0: u32,
    pub smbus_tx_eth_status1: u32,
    pub smbus_tx_pcie_status: u32,
    pub smbus_tx_faults: u32,
    pub smbus_tx_arc0_health: u32,
    pub smbus_tx_arc1_health: u32,
    pub smbus_tx_arc2_health: u32,
    pub smbus_tx_arc3_health: u32,
    pub smbus_tx_fan_speed: u32,
    pub smbus_tx_aiclk: u32,
    pub smbus_tx_axiclk: u32,
    pub smbus_tx_arcclk: u32,
    pub smbus_tx_throttler: u32,
    pub smbus_tx_vcore: u32,
    pub smbus_tx_asic_temperature: u32,
    pub smbus_tx_vreg_temperature: u32,
    pub smbus_tx_board_temperature: u32,
    pub smbus_tx_tdp: u32,
    pub smbus_tx_tdc: u32,
    pub smbus_tx_vdd_limits: u32,
    pub smbus_tx_thm_limits: u32,
    pub smbus_tx_wh_fw_date: u32,
    pub smbus_tx_asic_tmon0: u32,
    pub smbus_tx_asic_tmon1: u32,
    pub smbus_tx_mvddq_power: u32,
    pub smbus_tx_gddr_train_temp0: u32,
    pub smbus_tx_gddr_train_temp1: u32,
    pub smbus_tx_asic_power: Option<u32>,
    pub smbus_tx_aux_status: Option<u32>,
    pub smbus_tx_boot_date: u32,
    pub smbus_tx_rt_seconds: u32,
    pub smbus_tx_eth_debug_status0: u32,
    pub smbus_tx_eth_debug_status1: u32,
    pub smbus_tx_tt_flash_version: u32,
}

impl Telemetry {
    /// Return firmware date in YYYY-MM-DD format.
    pub fn firmware_date(&self) -> String {
        let year = (self.smbus_tx_wh_fw_date >> 28 & 0xF) + 2020;
        let month = (self.smbus_tx_wh_fw_date >> 24) & 0xF;
        let day = (self.smbus_tx_wh_fw_date >> 16) & 0xFF;
        let _hour = (self.smbus_tx_wh_fw_date >> 8) & 0xFF;
        let _minute = self.smbus_tx_wh_fw_date & 0xFF;
        format!("{:04}-{:02}-{:02}", year, month, day)
    }

    /// Return ARC firmware version in MAJOR.MINOR.PATCH format.
    pub fn arc_fw_version(&self) -> String {
        let major = (self.smbus_tx_arc0_fw_version >> 16) & 0xFF;
        let minor = (self.smbus_tx_arc0_fw_version >> 8) & 0xFF;
        let patch = (self.smbus_tx_arc0_fw_version >> 0) & 0xFF;
        format!("{}.{}.{}", major, minor, patch)
    }

    /// Return Ethernet firmware version in MAJOR.MINOR.PATCH format.
    pub fn eth_fw_version(&self) -> String {
        let major = (self.smbus_tx_eth_fw_version >> 16) & 0x0FF;
        let minor = (self.smbus_tx_eth_fw_version >> 12) & 0x00F;
        let patch = (self.smbus_tx_eth_fw_version >> 0) & 0xFFF;
        format!("{}.{}.{}", major, minor, patch)
    }

    /// Return the board serial number as an integer.
    pub fn board_serial_number(&self) -> u64 {
        ((self.smbus_tx_board_id_high as u64) << 32) | self.smbus_tx_board_id_low as u64
    }

    /// Return the board serial number as a hex-formatted string.
    pub fn board_serial_number_hex(&self) -> String {
        format!("{:016x}", self.board_serial_number())
    }

    /// Return the board type.
    pub fn board_type(&self) -> &'static str {
        let serial_num = self.board_serial_number();
        match (serial_num >> 36) & 0xFFFFF {
            0x1 => match (serial_num >> 32) & 0xF {
                0x2 => "E300_R2",
                0x3 | 0x4 => "E300_R3",
                _ => "UNSUPPORTED",
            },
            0x3 => "e150",
            0x7 => "e75",
            0x8 => "NEBULA_CB",
            0xA => "e300",
            0xB => "GALAXY",
            0x14 => "n300",
            0x18 => "n150",
            _ => "UNSUPPORTED",
        }
    }

    /// Return the AI clock speed in MHz.
    pub fn ai_clk(&self) -> u32 {
        self.smbus_tx_aiclk & 0xffff
    }

    /// Return the AXI clock speed in MHz.
    pub fn axi_clk(&self) -> u32 {
        self.smbus_tx_axiclk
    }

    /// Return the ARC clock speed in MHz.
    pub fn arc_clk(&self) -> u32 {
        self.smbus_tx_arcclk
    }

    /// Return the core voltage in volts.
    pub fn voltage(&self) -> f64 {
        self.smbus_tx_vcore as f64 / 1000.0
    }

    /// Return the ASIC temperature in degrees celsius.
    pub fn asic_temperature(&self) -> f64 {
        ((self.smbus_tx_asic_temperature & 0xffff) >> 4) as f64
    }

    /// Return the voltage regulator temperature in degrees celsius.
    pub fn vreg_temperature(&self) -> f64 {
        (self.smbus_tx_vreg_temperature & 0xffff) as f64
    }

    /// Return the inlet temperature in degrees celsius.
    pub fn inlet_temperature(&self) -> f64 {
        ((self.smbus_tx_board_temperature >> 0x10) & 0xff) as f64
    }

    /// Return the first outlet temperature in degrees celsius.
    pub fn outlet_temperature1(&self) -> f64 {
        ((self.smbus_tx_board_temperature >> 0x08) & 0xff) as f64
    }

    /// Return the second outlet temperature in degrees celsius.
    pub fn outlet_temperature2(&self) -> f64 {
        ((self.smbus_tx_board_temperature >> 0x00) & 0xff) as f64
    }

    /// Return the power consumption in watts.
    pub fn power(&self) -> f64 {
        (self.smbus_tx_tdp & 0xffff) as f64
    }

    /// Return the current consumption in amperes.
    pub fn current(&self) -> f64 {
        (self.smbus_tx_tdc & 0xffff) as f64
    }
}

pub enum ChipInitResult {
    /// Everything is good, can continue with init
    NoError,
    /// We hit an error, but we can continue with init
    /// this is for things like arc or ethernet training timeout.
    /// If this is returned then there shouldn't be a chip returned to the user,
    /// but we are okay to findout more information.
    ErrorContinue,
    /// We hit an error that indicates that it would be unsafe to continue with init.
    ErrorAbort,
}

/// Defines common functionality for all chips.
/// This is a convinence interface that allows chip type agnostic code to be written.
///
/// As a general rule the chip should not be accessed without an explicit request from the user.
/// This means that chip initialization must be explicity called and for example if the user has not
/// explicity stated that they want to enumerate remote chips, then we won't even start looking at remote readiness.
/// This is to avoid situations where a problematic state is reached and causes an abort even if that capability is not needed.
pub trait ChipImpl: HlComms + Send + Sync + 'static {
    /// Check if the chip has been initialized.
    /// This is also the starting point for the chip initialization process.
    fn is_inititalized(&self) -> Result<InitStatus, PlatformError>;

    /// Update the initialization state of the chip.
    /// The primary purpose of this function is to tell the caller when it is safe to starting interacting with the chip.
    ///
    /// However the secondary purpose is to provide information about what chip functions are currently available for use.
    /// For example if the arc is not ready, then we should not try to send an arc message.
    /// Or in a more complex example, if the arc is ready, but the ethernet is not (for example the ethernet fw is hung)
    /// then we will be able to access the local arc, but won't be able to access any remote chips.
    fn update_init_state(&self, status: &mut InitStatus) -> Result<ChipInitResult, PlatformError>;

    /// Returns the current arch of the chip, can be used to avoid
    /// needing to ducktype when downcasting.
    fn get_arch(&self) -> Arch;

    /// Get telemetry information from the chip.
    /// The information is not cached, so should not be called repeatedly.
    fn get_telemetry(&self) -> Result<Telemetry, PlatformError>;

    /// Send an arc_msg to the underlying chip.
    fn arc_msg(&self, msg: ArcMsgOptions) -> Result<ArcMsgOk, PlatformError>;

    /// Get a list of neighbouring chips.
    /// Will return an empty list for gs and up to four chips for wh.
    fn get_neighbouring_chips(&self) -> Result<Vec<NeighbouringChip>, PlatformError>;

    /// Convinence function to downcast to a concrete type.
    fn as_any(&self) -> &dyn std::any::Any;

    /// Get information about the underlying chip transport.
    /// This is a hack to get the physical id of the chip.
    fn get_device_info(&self) -> Result<Option<DeviceInfo>, PlatformError>;
}

/// A wrapper around a chip that implements `ChipImpl`.
/// This allows us to create and use chips without knowing their type,
/// but we can still downcast to the concrete type if we need to.
pub struct Chip {
    pub inner: Box<dyn ChipImpl>,
}

impl From<Box<dyn ChipImpl>> for Chip {
    fn from(inner: Box<dyn ChipImpl>) -> Self {
        Self { inner }
    }
}

impl Chip {
    /// Downcast to a wormhole chip
    pub fn as_wh(&self) -> Option<&Wormhole> {
        self.inner.as_any().downcast_ref::<Wormhole>()
    }

    /// Downcast to a grayskull chip
    pub fn as_gs(&self) -> Option<&Grayskull> {
        self.inner.as_any().downcast_ref::<Grayskull>()
    }
}

impl HlComms for Chip {
    fn comms_obj(&self) -> (&dyn ChipComms, &dyn ChipInterface) {
        self.inner.comms_obj()
    }
}

impl ChipImpl for Chip {
    fn is_inititalized(&self) -> Result<InitStatus, PlatformError> {
        self.inner.is_inititalized()
    }

    fn update_init_state(&self, status: &mut InitStatus) -> Result<ChipInitResult, PlatformError> {
        self.inner.update_init_state(status)
    }

    fn get_arch(&self) -> Arch {
        self.inner.get_arch()
    }

    fn arc_msg(&self, msg: ArcMsgOptions) -> Result<ArcMsgOk, PlatformError> {
        self.inner.arc_msg(msg)
    }

    fn get_neighbouring_chips(&self) -> Result<Vec<NeighbouringChip>, PlatformError> {
        self.inner.get_neighbouring_chips()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self.inner.as_any()
    }

    fn get_telemetry(&self) -> Result<Telemetry, PlatformError> {
        self.inner.get_telemetry()
    }

    fn get_device_info(&self) -> Result<Option<DeviceInfo>, PlatformError> {
        self.inner.get_device_info()
    }
}
