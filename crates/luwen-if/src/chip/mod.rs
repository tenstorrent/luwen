// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

mod blackhole;
pub mod communication;
mod creation;
pub mod eth_addr;
mod grayskull;
mod hl_comms;
mod init;
mod remote;
mod spi;
pub mod wh_ubb;
mod wormhole;

pub use blackhole::spirom_tables; // Added for proto file write unit testing
pub use blackhole::{message::MessageError, Blackhole};
pub use communication::chip_comms::{
    axi_translate, ArcIf, AxiData, AxiError, ChipComms, MemorySlice, MemorySlices,
};
pub use communication::chip_interface::{ChipInterface, NocInterface};
pub use grayskull::Grayskull;
pub use hl_comms::{HlComms, HlCommsInterface};
pub use init::status::InitStatus;
pub use init::{
    status::{CommsStatus, ComponentStatusInfo},
    wait_for_init, CallReason, ChipDetectState, InitError,
};
use luwen_core::Arch;
pub use wormhole::Wormhole;

use crate::arc_msg::TypedArcMsg;
pub use crate::arc_msg::{ArcMsg, ArcMsgOk};
use crate::{arc_msg::ArcMsgAddr, error::PlatformError, DeviceInfo};
pub use wh_ubb::{ubb_wait_for_driver_load, wh_ubb_ipmi_reset};

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
            msg: ArcMsg::Typed(TypedArcMsg::Nop),
            wait_for_done: true,
            timeout: std::time::Duration::from_secs(1),
            use_second_mailbox: false,
            addrs: None,
        }
    }
}

#[derive(Debug, Hash, PartialEq, Eq)]
pub struct NeighbouringChip {
    pub routing_enabled: bool,
    pub local_noc_addr: (u8, u8),
    pub remote_noc_addr: (u8, u8),
    pub eth_addr: crate::EthAddr,
}

#[derive(Default, Debug)]
pub struct Telemetry {
    pub arch: Arch,
    pub board_id: u64,
    pub enum_version: u32,
    pub entry_count: u32,
    pub device_id: u32,
    pub asic_id: u32,
    pub asic_ro: u32,
    pub asic_idd: u32,
    pub board_id_high: u32,
    pub board_id_low: u32,
    pub harvesting_state: u32,
    pub update_telem_speed: u32,
    pub arc0_fw_version: u32,
    pub arc1_fw_version: u32,
    pub arc2_fw_version: u32,
    pub arc3_fw_version: u32,
    pub spibootrom_fw_version: u32,
    pub eth_fw_version: u32,
    pub ddr_fw_version: u32,
    pub l2cpu_fw_version: u32,
    pub m3_bl_fw_version: u32,
    pub m3_app_fw_version: u32,
    pub ddr_speed: Option<u32>,
    pub ddr_status: u32,
    pub eth_status0: u32,
    pub eth_status1: u32,
    pub pcie_status: u32,
    pub faults: u32,
    pub arc0_health: u32,
    pub arc1_health: u32,
    pub arc2_health: u32,
    pub arc3_health: u32,
    pub fan_speed: u32,
    pub aiclk: u32,
    pub axiclk: u32,
    pub arcclk: u32,
    pub l2cpuclk0: u32,
    pub l2cpuclk1: u32,
    pub l2cpuclk2: u32,
    pub l2cpuclk3: u32,
    pub throttler: u32,
    pub vcore: u32,
    pub asic_temperature: u32,
    pub vreg_temperature: u32,
    pub board_temperature: u32,
    pub tdp: u32,
    pub tdc: u32,
    pub vdd_limits: u32,
    pub thm_limits: u32,
    pub wh_fw_date: u32,
    pub asic_tmon0: u32,
    pub asic_tmon1: u32,
    pub mvddq_power: u32,
    pub gddr_train_temp0: u32,
    pub gddr_train_temp1: u32,
    pub asic_power: Option<u32>,
    pub aux_status: Option<u32>,
    pub boot_date: u32,
    pub rt_seconds: u32,
    pub eth_debug_status0: u32,
    pub eth_debug_status1: u32,
    pub tt_flash_version: u32,
    pub fw_bundle_version: u32,
    pub timer_heartbeat: u32,
    pub noc_translation_enabled: bool,
    pub tensix_enabled_col: u32,
    pub enabled_eth: u32,
    pub enabled_gddr: u32,
    pub enabled_l2cpu: u32,
    pub enabled_pcie: u32,
    pub fan_rpm: u32,
    pub gddr01_temp: u32,
    pub gddr23_temp: u32,
    pub gddr45_temp: u32,
    pub gddr67_temp: u32,
    pub gddr01_corr_errs: u32,
    pub gddr23_corr_errs: u32,
    pub gddr45_corr_errs: u32,
    pub gddr67_corr_errs: u32,
    pub gddr_uncorr_errs: u32,
    pub max_gddr_temp: u32,
    pub asic_location: u32,
    pub board_power_limit: u32,
    pub input_power: u32,
    pub therm_trip_count: u32,
    pub asic_id_high: u32,
    pub asic_id_low: u32,
}

impl Telemetry {
    /// Return firmware date in YYYY-MM-DD format.
    pub fn firmware_date(&self) -> String {
        let year = ((self.wh_fw_date >> 28) & 0xF) + 2020;
        let month = (self.wh_fw_date >> 24) & 0xF;
        let day = (self.wh_fw_date >> 16) & 0xFF;
        let _hour = (self.wh_fw_date >> 8) & 0xFF;
        let _minute = self.wh_fw_date & 0xFF;
        format!("{year:04}-{month:02}-{day:02}")
    }

    /// Return ARC firmware version in MAJOR.MINOR.PATCH format.
    pub fn arc_fw_version(&self) -> String {
        let major = (self.arc0_fw_version >> 16) & 0xFF;
        let minor = (self.arc0_fw_version >> 8) & 0xFF;
        let patch = self.arc0_fw_version & 0xFF;
        format!("{major}.{minor}.{patch}")
    }

    /// Return Ethernet firmware version in MAJOR.MINOR.PATCH format.
    pub fn eth_fw_version(&self) -> String {
        let major = (self.eth_fw_version >> 16) & 0x0FF;
        let minor = (self.eth_fw_version >> 12) & 0x00F;
        let patch = self.eth_fw_version & 0xFFF;
        format!("{major}.{minor}.{patch}")
    }

    /// Return the board serial number as an integer.
    pub fn board_serial_number(&self) -> u64 {
        ((self.board_id_high as u64) << 32) | self.board_id_low as u64
    }

    /// Return the board serial number as a hex-formatted string.
    pub fn board_serial_number_hex(&self) -> String {
        format!("{:016x}", self.board_serial_number())
    }

    /// Return the board type or None if unknown
    pub fn try_board_type(&self) -> Option<&'static str> {
        let serial_num = self.board_serial_number();
        let output = match (serial_num >> 36) & 0xFFFFF {
            0x1 => match (serial_num >> 32) & 0xF {
                0x2 => "E300_R2",
                0x3 | 0x4 => "E300_R3",
                _ => return None,
            },
            0x3 => "e150",
            0x7 => "e75",
            0x8 => "NEBULA_CB",
            0xA => "e300",
            0xB => "GALAXY",
            0x14 => "n300",
            0x18 => "n150",
            0x35 => "galaxy-wormhole",
            0x36 => "p100",
            0x40 => "p150a",
            0x41 => "p150b",
            0x42 => "p150c",
            0x43 => "p100a",
            0x44 => "p300b",
            0x45 => "p300a",
            0x46 => "p300c",
            0x47 => "galaxy-blackhole",
            _ => return None,
        };

        Some(output)
    }

    /// Return the board type of UNSUPPORTED
    pub fn board_type(&self) -> &'static str {
        self.try_board_type().unwrap_or("UNSUPPORTED")
    }

    /// Return the AI clock speed in MHz.
    pub fn ai_clk(&self) -> u32 {
        self.aiclk & 0xffff
    }

    /// Return the AXI clock speed in MHz.
    pub fn axi_clk(&self) -> u32 {
        self.axiclk
    }

    /// Return the ARC clock speed in MHz.
    pub fn arc_clk(&self) -> u32 {
        self.arcclk
    }

    /// Return the core voltage in volts.
    pub fn voltage(&self) -> f64 {
        self.vcore as f64 / 1000.0
    }

    /// Return the ASIC temperature in degrees celsius.
    pub fn asic_temperature(&self) -> f64 {
        if self.arch.is_blackhole() {
            let frac: f64 = (self.asic_temperature & 0xFFFF).into();
            let frac = frac / 65536.0;

            let int: f64 = (self.asic_temperature >> 16).into();

            int + frac
        } else {
            ((self.asic_temperature & 0xffff) >> 4) as f64
        }
    }

    /// Return the voltage regulator temperature in degrees celsius.
    pub fn vreg_temperature(&self) -> f64 {
        (self.vreg_temperature & 0xffff) as f64
    }

    /// Return the inlet temperature in degrees celsius.
    pub fn inlet_temperature(&self) -> f64 {
        ((self.board_temperature >> 0x10) & 0xff) as f64
    }

    /// Return the first outlet temperature in degrees celsius.
    pub fn outlet_temperature1(&self) -> f64 {
        ((self.board_temperature >> 0x08) & 0xff) as f64
    }

    /// Return the second outlet temperature in degrees celsius.
    pub fn outlet_temperature2(&self) -> f64 {
        (self.board_temperature & 0xff) as f64
    }

    /// Return the power consumption in watts.
    pub fn power(&self) -> f64 {
        (self.tdp & 0xffff) as f64
    }

    /// Return the current consumption in amperes.
    pub fn current(&self) -> f64 {
        (self.tdc & 0xffff) as f64
    }

    pub fn telemetry_heartbeat(&self) -> u32 {
        if self.arch.is_blackhole() {
            self.timer_heartbeat
        } else {
            self.arc0_health
        }
    }
}

pub enum ChipInitResult {
    /// Everything is good, can continue with init
    NoError,
    /// We hit an error, but we can continue with init
    /// this is for things like arc or ethernet training timeout.
    /// If this is returned then there shouldn't be a chip returned to the user,
    /// but we are okay to findout more information.
    ErrorContinue(String, std::backtrace::Backtrace),
    /// We hit an error that indicates that it would be unsafe to continue with init.
    ErrorAbort(String, std::backtrace::Backtrace),
}

/// Defines common functionality for all chips.
/// This is a convinence interface that allows chip type agnostic code to be written.
///
/// As a general rule the chip should not be accessed without an explicit request from the user.
/// This means that chip initialization must be explicity called and for example if the user has not
/// explicity stated that they want to enumerate remote chips, then we won't even start looking at remote readiness.
/// This is to avoid situations where a problematic state is reached and causes an abort even if that capability is not needed.
pub trait ChipImpl: HlComms + Send + Sync + 'static {
    /// Update the initialization state of the chip.
    /// The primary purpose of this function is to tell the caller when it is safe to starting interacting with the chip.
    ///
    /// However the secondary purpose is to provide information about what chip functions are currently available for use.
    /// For example if the arc is not ready, then we should not try to send an arc message.
    /// Or in a more complex example, if the arc is ready, but the ethernet is not (for example the ethernet fw is hung)
    /// then we will be able to access the local arc, but won't be able to access any remote chips.
    fn update_init_state(
        &mut self,
        status: &mut InitStatus,
    ) -> Result<ChipInitResult, PlatformError>;

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

    /// Downcast to a blackhole chip
    pub fn as_bh(&self) -> Option<&Blackhole> {
        self.inner.as_any().downcast_ref::<Blackhole>()
    }
}

impl HlComms for Chip {
    fn comms_obj(&self) -> (&dyn ChipComms, &dyn ChipInterface) {
        self.inner.comms_obj()
    }
}

impl ChipImpl for Chip {
    fn update_init_state(
        &mut self,
        status: &mut InitStatus,
    ) -> Result<ChipInitResult, PlatformError> {
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
