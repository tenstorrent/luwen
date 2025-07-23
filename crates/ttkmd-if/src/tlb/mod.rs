// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{PciDevice, PciError};

mod blackhole;
mod grayskull;
mod wormhole;

#[derive(Clone, Debug, Default, Hash, PartialEq)]
#[repr(u8)]
pub enum Ordering {
    RELAXED = 0,
    #[default]
    STRICT = 1,
    POSTED = 2,
    PostedStrict = 3,
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
            Ordering::PostedStrict => 3,
            Ordering::UNKNOWN(val) => val,
        }
    }
}

#[derive(Clone, Debug, Hash, PartialEq)]
pub struct TlbStride {
    pub stride_x: u8,
    pub stride_y: u8,
    pub quad_exclude_x: u8,
    pub quad_exclude_y: u8,
    pub quad_exclude_control: u8,
    pub num_destinations: u8,
}

#[derive(Debug, Clone, Default)]
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
    pub use_static_vc: bool,
    pub stream_header: bool,
    pub static_vc: u8,

    pub stride: Option<TlbStride>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MemoryType {
    Uc,
    Wc,
}

#[derive(Debug)]
pub struct TlbInfo {
    pub count: u64,
    pub size: u64,
    pub memory_type: MemoryType,
}

pub struct DeviceTlbInfo {
    pub device_id: u32,
    pub total_count: u32,
    pub tlb_config: Vec<TlbInfo>,
}

#[derive(Debug)]
pub struct SpecificTlbInfo {
    pub config_base: u64,
    pub data_base: u64,
    pub size: u64,
    pub memory_type: MemoryType,
}

pub fn get_per_tlb_info(device: &PciDevice, index: u32) -> SpecificTlbInfo {
    match device.arch {
        crate::Arch::Grayskull => grayskull::get_specific_tlb_info(device, index),
        crate::Arch::Wormhole => wormhole::get_specific_tlb_info(device, index),
        crate::Arch::Blackhole => blackhole::get_specific_tlb_info(device, index),
    }
}

pub fn get_tlb(device: &PciDevice, index: u32) -> Result<Tlb, PciError> {
    match device.arch {
        crate::Arch::Grayskull => grayskull::get_tlb(device, index),
        crate::Arch::Wormhole => wormhole::get_tlb(device, index),
        crate::Arch::Blackhole => blackhole::get_tlb(device, index),
    }
}

pub fn setup_tlb(device: &mut PciDevice, index: u32, tlb: Tlb) -> Result<(u64, u64), PciError> {
    match device.arch {
        crate::Arch::Grayskull => grayskull::setup_tlb(device, index, tlb),
        crate::Arch::Wormhole => wormhole::setup_tlb(device, index, tlb),
        crate::Arch::Blackhole => blackhole::setup_tlb(device, index, tlb),
    }
}

pub fn get_tlb_info(device: &PciDevice) -> DeviceTlbInfo {
    match device.arch {
        crate::Arch::Grayskull => grayskull::tlb_info(device),
        crate::Arch::Wormhole => wormhole::tlb_info(device),
        crate::Arch::Blackhole => blackhole::tlb_info(device),
    }
}
