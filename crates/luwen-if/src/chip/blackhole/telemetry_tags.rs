use crate::{error::PlatformError};

use super::Blackhole;

#[repr(u32)]
pub enum TelemetryTags {
    BOARD_ID_HIGH = 1,
    BOARD_ID_LOW = 2,
    ASIC_ID = 3,
    HARVESTING_STATE = 4,
    UPDATE_TELEM_SPEED = 5,
    VCORE = 6,
    TDP = 7,
    TDC = 8,
    VDD_LIMITS = 9,
    THM_LIMITS = 10,
    ASIC_TEMPERATURE = 11,
    VREG_TEMPERATURE = 12,
    BOARD_TEMPERATURE = 13,
    AICLK = 14,
    AXICLK = 15,
    ARCCLK = 16,
    L2CPUCLK0 = 17,
    L2CPUCLK1 = 18,
    L2CPUCLK2 = 19,
    L2CPUCLK3 = 20,
    ETH_LIVE_STATUS = 21,
    DDR_STATUS = 22,
    DDR_SPEED = 23,
    ETH_FW_VERSION = 24,
    DDR_FW_VERSION = 25,
    BM_APP_FW_VERSION = 26,
    BM_BL_FW_VERSION = 27,
    FLASH_BUNDLE_VERSION = 28,
    CM_FW_VERSION = 29,
    L2CPU_FW_VERSION = 30,
    FAN_SPEED = 31,
    TIMER_HEARTBEAT = 32,
    TELEM_ENUM_COUNT = 33,
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
            1 => TelemetryTags::BOARD_ID_HIGH,
            2 => TelemetryTags::BOARD_ID_LOW,
            3 => TelemetryTags::ASIC_ID,
            4 => TelemetryTags::HARVESTING_STATE,
            5 => TelemetryTags::UPDATE_TELEM_SPEED,
            6 => TelemetryTags::VCORE,
            7 => TelemetryTags::TDP,
            8 => TelemetryTags::TDC,
            9 => TelemetryTags::VDD_LIMITS,
            10 => TelemetryTags::THM_LIMITS,
            11 => TelemetryTags::ASIC_TEMPERATURE,
            12 => TelemetryTags::VREG_TEMPERATURE,
            13 => TelemetryTags::BOARD_TEMPERATURE,
            14 => TelemetryTags::AICLK,
            15 => TelemetryTags::AXICLK,
            16 => TelemetryTags::ARCCLK,
            17 => TelemetryTags::L2CPUCLK0,
            18 => TelemetryTags::L2CPUCLK1,
            19 => TelemetryTags::L2CPUCLK2,
            20 => TelemetryTags::L2CPUCLK3,
            21 => TelemetryTags::ETH_LIVE_STATUS,
            22 => TelemetryTags::DDR_STATUS,
            23 => TelemetryTags::DDR_SPEED,
            24 => TelemetryTags::ETH_FW_VERSION,
            25 => TelemetryTags::DDR_FW_VERSION,
            26 => TelemetryTags::BM_APP_FW_VERSION,
            27 => TelemetryTags::BM_BL_FW_VERSION,
            28 => TelemetryTags::FLASH_BUNDLE_VERSION,
            29 => TelemetryTags::CM_FW_VERSION,
            30 => TelemetryTags::L2CPU_FW_VERSION,
            31 => TelemetryTags::FAN_SPEED,
            32 => TelemetryTags::TIMER_HEARTBEAT,
            33 => TelemetryTags::TELEM_ENUM_COUNT,
            _ => panic!("Invalid telemetry tag value"),
        }
    };
}