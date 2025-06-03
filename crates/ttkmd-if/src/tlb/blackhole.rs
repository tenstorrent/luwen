// SPDX-FileCopyrightText: © 2024 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    tlb::{MemoryType, SpecificTlbInfo, TlbInfo},
    DeviceTlbInfo, PciDevice, PciError, Tlb,
};

use super::Ordering;

#[bitfield_struct::bitfield(u128)]
pub struct Tlb2M {
    #[bits(43)]
    local_offset: u64,
    #[bits(6)]
    x_end: u8,
    #[bits(6)]
    y_end: u8,
    #[bits(6)]
    x_start: u8,
    #[bits(6)]
    y_start: u8,
    #[bits(2)]
    noc_sel: u8,
    mcast: bool,
    #[bits(2)]
    ordering: u8,
    linked: bool,
    use_static_vc: bool,
    stream_header: bool,
    #[bits(3)]
    static_vc: u8,
    #[bits(50)]
    padding: u64,
}

#[bitfield_struct::bitfield(u32)]
pub struct Tlb2MStride {
    #[bits(4)]
    stride_x: u8,
    #[bits(4)]
    stride_y: u8,
    #[bits(5)]
    quad_exclude_x: u8,
    #[bits(4)]
    quad_exclude_y: u8,
    #[bits(4)]
    quad_exclude_ctl: u8,
    num_destinations: u8,
    #[bits(3)]
    padding: u32,
}

impl From<Tlb> for (Tlb2M, Tlb2MStride) {
    fn from(value: Tlb) -> Self {
        assert!(!value.mcast || (!matches!(value.ordering, Ordering::PostedStrict)));

        let mut stride = Tlb2MStride::new();
        if let Some(value) = value.stride {
            stride.set_stride_x(value.stride_x);
        }

        (
            Tlb2M::new()
                .with_local_offset(value.local_offset)
                .with_x_end(value.x_end)
                .with_y_end(value.y_end)
                .with_x_start(value.x_start)
                .with_y_start(value.y_start)
                .with_noc_sel(value.noc_sel)
                .with_mcast(value.mcast)
                .with_ordering(value.ordering.into())
                .with_linked(value.linked),
            stride,
        )
    }
}

impl From<(Tlb2M, Option<Tlb2MStride>)> for Tlb {
    fn from(value: (Tlb2M, Option<Tlb2MStride>)) -> Self {
        Tlb {
            local_offset: value.0.local_offset(),
            x_end: value.0.x_end(),
            y_end: value.0.y_end(),
            x_start: value.0.x_start(),
            y_start: value.0.y_start(),
            noc_sel: value.0.noc_sel(),
            mcast: value.0.mcast(),
            ordering: Ordering::from(value.0.ordering()),
            linked: value.0.linked(),

            use_static_vc: value.0.use_static_vc(),
            stream_header: value.0.stream_header(),
            static_vc: value.0.static_vc(),

            stride: value.1.map(|v| super::TlbStride {
                stride_x: v.stride_x(),
                stride_y: v.stride_y(),
                quad_exclude_x: v.quad_exclude_x(),
                quad_exclude_y: v.quad_exclude_y(),
                quad_exclude_control: v.quad_exclude_ctl(),
                num_destinations: v.num_destinations(),
            }),
        }
    }
}

impl From<Tlb2M> for Tlb {
    fn from(value: Tlb2M) -> Self {
        Tlb {
            local_offset: value.local_offset(),
            x_end: value.x_end(),
            y_end: value.y_end(),
            x_start: value.x_start(),
            y_start: value.y_start(),
            noc_sel: value.noc_sel(),
            mcast: value.mcast(),
            ordering: Ordering::from(value.ordering()),
            linked: value.linked(),

            use_static_vc: value.use_static_vc(),
            stream_header: value.stream_header(),
            static_vc: value.static_vc(),

            stride: None,
        }
    }
}

#[bitfield_struct::bitfield(u128)]
pub struct Tlb4G {
    local_offset: u32,
    #[bits(6)]
    x_end: u8,
    #[bits(6)]
    y_end: u8,
    #[bits(6)]
    x_start: u8,
    #[bits(6)]
    y_start: u8,
    #[bits(2)]
    noc_sel: u8,
    mcast: bool,
    #[bits(2)]
    ordering: u8,
    linked: bool,

    use_static_vc: bool,
    stream_header: bool,
    #[bits(3)]
    static_vc: u8,

    #[bits(61)]
    padding: u64,
}

impl From<Tlb4G> for Tlb {
    fn from(value: Tlb4G) -> Self {
        Tlb {
            local_offset: value.local_offset() as u64,
            x_end: value.x_end(),
            y_end: value.y_end(),
            x_start: value.x_start(),
            y_start: value.y_start(),
            noc_sel: value.noc_sel(),
            mcast: value.mcast(),
            ordering: Ordering::from(value.ordering()),
            linked: value.linked(),

            use_static_vc: value.use_static_vc(),
            stream_header: value.stream_header(),
            static_vc: value.static_vc(),

            stride: None,
        }
    }
}

impl From<Tlb> for Tlb4G {
    fn from(value: Tlb) -> Self {
        assert!(!value.mcast || (!matches!(value.ordering, Ordering::PostedStrict)));

        Self::new()
            .with_local_offset(value.local_offset as u32)
            .with_x_end(value.x_end)
            .with_y_end(value.y_end)
            .with_x_start(value.x_start)
            .with_y_start(value.y_start)
            .with_noc_sel(value.noc_sel)
            .with_mcast(value.mcast)
            .with_ordering(value.ordering.into())
            .with_linked(value.linked)
    }
}

pub fn setup_tlb(
    device: &mut PciDevice,
    tlb_index: u32,
    mut tlb: Tlb,
) -> Result<(u64, u64), PciError> {
    const TLB_CONFIG_BASE: u64 = 0x1FC00000;
    const TLB_CONFIG_SIZE: u64 = (32 * 3) / 8;

    const TLB_COUNT_2M: u64 = 202;
    const TLB_COUNT_4G: u64 = 8;
    const STRIDED_COUNT: u64 = 32;

    const TLB_INDEX_2M: u64 = 0;
    const TLB_END_2M: u64 = TLB_INDEX_2M + TLB_COUNT_2M - 1;
    const TLB_INDEX_4G: u64 = TLB_COUNT_2M;
    const TLB_END_4G: u64 = TLB_INDEX_4G + TLB_COUNT_4G - 1;

    const TLB_BASE_2M: u64 = 0;
    const TLB_BASE_4G: u64 = TLB_COUNT_2M * (1 << 32);

    let tlb_config_addr = TLB_CONFIG_BASE + (tlb_index as u64 * TLB_CONFIG_SIZE);

    let (tlb_value, stride_programming, mmio_addr, size, addr_offset) = match tlb_index as u64 {
        TLB_INDEX_2M..=TLB_END_2M => {
            let size = 1 << 21;
            let tlb_address = tlb.local_offset / size;
            let local_offset = tlb.local_offset % size;

            tlb.local_offset = tlb_address;
            let (tlb, stride) = <(Tlb2M, Tlb2MStride)>::from(tlb);
            (
                tlb.0,
                if (TLB_INDEX_2M..(TLB_INDEX_2M + STRIDED_COUNT)).contains(&(tlb_index as u64)) {
                    Some(stride)
                } else {
                    None
                },
                TLB_BASE_2M + size * tlb_index as u64,
                size,
                local_offset,
            )
        }
        TLB_INDEX_4G..=TLB_END_4G => {
            let size = 1 << 32;
            let tlb_address = tlb.local_offset / size;
            let local_offset = tlb.local_offset % size;

            tlb.local_offset = tlb_address;
            (
                Tlb4G::from(tlb).0,
                None,
                TLB_BASE_4G + size * (tlb_index - 156) as u64,
                size,
                local_offset,
            )
        }
        _ => {
            panic!("TLB index out of range");
        }
    };

    device.write32(tlb_config_addr as u32, (tlb_value & 0xFFFF_FFFF) as u32)?;
    device.write32(
        tlb_config_addr as u32 + 4,
        ((tlb_value >> 32) & 0xFFFF_FFFF) as u32,
    )?;
    device.write32(
        tlb_config_addr as u32 + 8,
        ((tlb_value >> 64) & 0xFFFF_FFFF) as u32,
    )?;

    if let Some(stride) = stride_programming {
        device.write32(
            (tlb_config_addr + (TLB_END_4G * TLB_CONFIG_SIZE)) as u32,
            stride.0,
        )?;
    }

    Ok((mmio_addr + addr_offset, size - addr_offset))
}

pub fn get_tlb(device: &PciDevice, tlb_index: u32) -> Result<Tlb, PciError> {
    const TLB_CONFIG_BASE: u32 = 0x1FC00000;
    const TLB_CONFIG_SIZE: u32 = (32 * 3) / 8;

    let tlb_config_addr = TLB_CONFIG_BASE + (tlb_index * TLB_CONFIG_SIZE);

    let tlb = ((device.read32(tlb_config_addr + 8)? as u128) << 64)
        | ((device.read32(tlb_config_addr + 4)? as u128) << 32)
        | (device.read32(tlb_config_addr)? as u128);

    let output = match tlb_index {
        0..=31 => {
            let tlb_stride = TLB_CONFIG_BASE + 0x9D8 + (tlb_index * 4);

            let tlb_base = Tlb2M::from(tlb);
            let tlb_stride = Tlb2MStride::from(tlb_stride);
            (tlb_base, Some(tlb_stride)).into()
        }
        32..=201 => Tlb2M::from(tlb).into(),
        202..=209 => Tlb4G::from(tlb).into(),
        _ => {
            panic!("TLB index out of range");
        }
    };

    Ok(output)
}

pub fn get_specific_tlb_info(device: &PciDevice, tlb_index: u32) -> SpecificTlbInfo {
    const TLB_CONFIG_BASE: u64 = 0x1FC00000;
    const TLB_CONFIG_SIZE: u64 = (32 * 3) / 8;

    const TLB_COUNT_2M: u64 = 202;
    const TLB_COUNT_4G: u64 = 8;
    const STRIDED_COUNT: u64 = 32;

    const TLB_INDEX_2M: u64 = 0;
    const TLB_END_2M: u64 = TLB_INDEX_2M + TLB_COUNT_2M - 1;
    const TLB_INDEX_4G: u64 = TLB_COUNT_2M;
    const TLB_END_4G: u64 = TLB_INDEX_4G + TLB_COUNT_4G - 1;

    const TLB_BASE_2M: u64 = 0;
    const TLB_BASE_4G: u64 = TLB_COUNT_2M * (1 << 32);

    let tlb_config_addr = TLB_CONFIG_BASE + (tlb_index as u64 * TLB_CONFIG_SIZE);
    let (_strided, tlb_data_addr, size) = match tlb_index as u64 {
        TLB_INDEX_2M..=TLB_END_2M => {
            let size = 1 << 21;

            (
                (TLB_INDEX_2M..(TLB_INDEX_2M + STRIDED_COUNT)).contains(&(tlb_index as u64)),
                TLB_BASE_2M + size * tlb_index as u64,
                size,
            )
        }
        TLB_INDEX_4G..=TLB_END_4G => {
            let size = 1 << 32;

            (false, TLB_BASE_4G + size * (tlb_index - 156) as u64, size)
        }
        _ => {
            panic!("TLB index out of range");
        }
    };

    let memory_type = device
        .pci_bar
        .as_ref()
        .and_then(|v| {
            if v.bar0_wc_size > tlb_data_addr {
                Some(MemoryType::Wc)
            } else {
                None
            }
        })
        .unwrap_or(MemoryType::Uc);

    SpecificTlbInfo {
        config_base: tlb_config_addr,
        data_base: tlb_data_addr,
        size,
        memory_type,
    }
}

pub fn tlb_info(device: &PciDevice) -> DeviceTlbInfo {
    const TLB_COUNT_2M: u64 = 202;
    const TLB_COUNT_4G: u64 = 8;

    DeviceTlbInfo {
        device_id: device.id as u32,
        total_count: 210,
        tlb_config: vec![
            TlbInfo {
                count: TLB_COUNT_2M,
                size: 1 << 21,
                memory_type: MemoryType::Uc,
            },
            TlbInfo {
                count: TLB_COUNT_4G,
                size: 1 << 32,
                memory_type: MemoryType::Uc,
            },
        ],
    }
}
