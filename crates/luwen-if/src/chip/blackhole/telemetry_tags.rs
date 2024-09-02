use crate::error::PlatformError;
use num_derive::FromPrimitive;

use super::Blackhole;

#[derive(FromPrimitive)]
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
