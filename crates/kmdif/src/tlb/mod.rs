use crate::{PciDevice, PciError};

mod grayskull;
mod wormhole;

#[derive(Default)]
#[repr(u8)]
pub enum Ordering {
    RELAXED = 0,
    STRICT = 1,
    #[default]
    POSTED = 2,
    UNKNOWN(u8),
}

impl From<u8> for Ordering {
    fn from(value: u8) -> Self {
        match value {
            0 => Self::RELAXED,
            1 => Self::STRICT,
            2 => Self::POSTED,
            val => Self::UNKNOWN(val),
        }
    }
}

impl From<Ordering> for u8 {
    fn from(value: Ordering) -> Self {
        match value {
            Ordering::RELAXED => 0,
            Ordering::STRICT => 1,
            Ordering::POSTED => 2,
            Ordering::UNKNOWN(val) => val,
        }
    }
}

#[derive(Default)]
pub struct Tlb {
    pub local_offset: u64,
    pub x_end: u8,
    pub y_end: u8,
    pub x_start: u8,
    pub y_start: u8,
    pub noc_sel: u8,
    pub mcast: bool,
    pub ordering: Ordering,
    pub linked: bool,
}

pub enum MemoryType {
    Uc,
    Wc,
}

pub struct TlbInfo {
    pub count: u64,
    pub size: u32,
    pub memory_type: MemoryType,
}

pub struct DeviceTlbInfo {
    pub device_id: u32,
    pub total_count: u32,
    pub tlb_config: Vec<TlbInfo>,
}

pub fn get_tlb(device: &mut PciDevice, index: u32) -> Result<Tlb, PciError> {
    match device.arch {
        crate::Arch::Grayskull => grayskull::get_tlb(device, index),
        crate::Arch::Wormhole => wormhole::get_tlb(device, index),
        crate::Arch::Unknown(_) => todo!(),
    }
}

pub fn setup_tlb(device: &mut PciDevice, index: u32, tlb: Tlb) -> Result<(u64, u64), PciError> {
    match device.arch {
        crate::Arch::Grayskull => grayskull::setup_tlb(device, index, tlb),
        crate::Arch::Wormhole => wormhole::setup_tlb(device, index, tlb),
        crate::Arch::Unknown(_) => todo!(),
    }
}

pub fn get_tlb_info(device: &PciDevice) -> DeviceTlbInfo {
    match device.arch {
        crate::Arch::Grayskull => grayskull::tlb_info(device),
        crate::Arch::Wormhole => wormhole::tlb_info(device),
        crate::Arch::Unknown(_) => todo!(),
    }
}
