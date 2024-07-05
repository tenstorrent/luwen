use crate::{error::PlatformError};

use super::Blackhole;

#[repr(u32)]
pub enum TelemetryTags {
    ENUM_VERSION = 0x0,
    ENTRY_COUNT = 0x1,
    BOARD_ID_HIGH = 0x2,
    BOARD_ID_LOW = 0x3,
    ASIC_ID = 0x4,
    HARVESTING_STATE = 0x5,
    UPDATE_TELEM_SPEED = 0x6,
    VCORE = 0x7,
    TDP = 0x8,
    TDC = 0x9,
    VDD_LIMITS = 0xA,
    THM_LIMITS = 0xB,
    ASIC_TEMPERATURE = 0xC,
    VREG_TEMPERATURE = 0xD,
    BOARD_TEMPERATURE = 0xE,
    AICLK = 0xF,
    AXICLK = 0x10,
    ARCCLK = 0x11,
    L2CPUCLK0 = 0x12,
    L2CPUCLK1 = 0x13,
    L2CPUCLK2 = 0x14,
    L2CPUCLK3 = 0x15,
    ETH_LIVE_STATUS = 0x16,
    DDR_STATUS = 0x17,
    DDR_SPEED = 0x18,
    ETH_FW_VERSION = 0x19,
    DDR_FW_VERSION = 0x1A,
    BM_APP_FW_VERSION = 0x1B,
    BM_BL_FW_VERSION = 0x1C,
    FLASH_BUNDLE_VERSION = 0x1D,
    CM_FW_VERSION = 0x1E,
    L2CPU_FW_VERSION = 0x1F,
    FAN_SPEED = 0x20,
    TIMER_HEARTBEAT = 0x21,
    TELEM_ENUM_COUNT = 0x22,
}

#[macro_export]
macro_rules! telemetry_tags_to_u32 {
    ($TelemetryTags:expr) => {
        $TelemetryTags as u32
    };
}

macro_rules! u32_to_telemetry_tags {
    ($value:expr) => {
        match $value {
            0x0 => TelemetryTags::ENUM_VERSION,
            0x1 => TelemetryTags::ENTRY_COUNT,
            0x2 => TelemetryTags::BOARD_ID_HIGH,
            0x3 => TelemetryTags::BOARD_ID_LOW,
            0x4 => TelemetryTags::ASIC_ID,
            0x5 => TelemetryTags::HARVESTING_STATE,
            0x6 => TelemetryTags::UPDATE_TELEM_SPEED,
            0x7 => TelemetryTags::VCORE,
            0x8 => TelemetryTags::TDP,
            0x9 => TelemetryTags::TDC,
            0xA => TelemetryTags::VDD_LIMITS,
            0xB => TelemetryTags::THM_LIMITS,
            0xC => TelemetryTags::ASIC_TEMPERATURE,
            0xD => TelemetryTags::VREG_TEMPERATURE,
            0xE => TelemetryTags::BOARD_TEMPERATURE,
            0xF => TelemetryTags::AICLK,
            0x10 => TelemetryTags::AXICLK,
            0x11 => TelemetryTags::ARCCLK,
            0x12 => TelemetryTags::L2CPUCLK0,
            0x13 => TelemetryTags::L2CPUCLK1,
            0x14 => TelemetryTags::L2CPUCLK2,
            0x15 => TelemetryTags::L2CPUCLK3,
            0x16 => TelemetryTags::ETH_LIVE_STATUS,
            0x17 => TelemetryTags::DDR_STATUS,
            0x18 => TelemetryTags::DDR_SPEED,
            0x19 => TelemetryTags::ETH_FW_VERSION,
            0x1A => TelemetryTags::DDR_FW_VERSION,
            0x1B => TelemetryTags::BM_APP_FW_VERSION,
            0x1C => TelemetryTags::BM_BL_FW_VERSION,
            0x1D => TelemetryTags::FLASH_BUNDLE_VERSION,
            0x1E => TelemetryTags::CM_FW_VERSION,
            0x1F => TelemetryTags::L2CPU_FW_VERSION,
            0x20 => TelemetryTags::FAN_SPEED,
            0x21 => TelemetryTags::TIMER_HEARTBEAT,
            0x22 => TelemetryTags::TELEM_ENUM_COUNT,
            _ => panic!("Invalid telemetry tag value"),
        }
    };
}