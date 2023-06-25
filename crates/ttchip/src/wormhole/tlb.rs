use kmdif::PciError;

use crate::common::Chip;

#[derive(Default)]
#[repr(u8)]
pub enum Ordering {
    RELAXED = 0,
    STRICT = 1,
    #[default]
    POSTED = 2,
}

#[derive(Default)]
pub struct Tlb {
    pub local_offset: u64,
    pub x_end: u8,
    pub y_end: u8,
    pub x_start: u8,
    pub y_start: u8,
    pub noc_sel: bool,
    pub mcast: bool,
    pub ordering: Ordering,
    pub linked: bool
}

#[bitfield_struct::bitfield(u64)]
pub struct Tlb1M {
    local_offset: u16,
    #[bits(6)]
    x_end: u64,
    #[bits(6)]
    y_end: u64,
    #[bits(6)]
    x_start: u64,
    #[bits(6)]
    y_start: u64,
    noc_sel: bool,
    mcast: bool,
    #[bits(2)]
    ordering: u64,
    linked: bool,
    #[bits(19)]
    padding: u64
}

impl From<Tlb> for Tlb1M {
    fn from(value: Tlb) -> Self {
        Self::new().with_local_offset(value.local_offset as u16)
            .with_x_end(value.x_end as u64)
            .with_y_end(value.y_end as u64)
            .with_x_start(value.x_start as u64)
            .with_y_start(value.y_start as u64)
            .with_noc_sel(value.noc_sel)
            .with_mcast(value.mcast)
            .with_ordering(value.ordering as u64)
            .with_linked(value.linked)
    }
}


#[bitfield_struct::bitfield(u64)]
pub struct Tlb2M {
    #[bits(15)]
    local_offset: u64,
    #[bits(6)]
    x_end: u64,
    #[bits(6)]
    y_end: u64,
    #[bits(6)]
    x_start: u64,
    #[bits(6)]
    y_start: u64,
    noc_sel: bool,
    mcast: bool,
    #[bits(2)]
    ordering: u64,
    linked: bool,
    #[bits(20)]
    padding: u64
}

impl From<Tlb> for Tlb2M {
    fn from(value: Tlb) -> Self {
        Self::new().with_local_offset(value.local_offset as u64)
            .with_x_end(value.x_end as u64)
            .with_y_end(value.y_end as u64)
            .with_x_start(value.x_start as u64)
            .with_y_start(value.y_start as u64)
            .with_noc_sel(value.noc_sel)
            .with_mcast(value.mcast)
            .with_ordering(value.ordering as u64)
            .with_linked(value.linked)
    }
}

#[bitfield_struct::bitfield(u64)]
pub struct Tlb16M {
    #[bits(12)]
    local_offset: u64,
    #[bits(6)]
    x_end: u64,
    #[bits(6)]
    y_end: u64,
    #[bits(6)]
    x_start: u64,
    #[bits(6)]
    y_start: u64,
    noc_sel: bool,
    mcast: bool,
    #[bits(2)]
    ordering: u64,
    linked: bool,
    #[bits(23)]
    padding: u64
}

impl From<Tlb> for Tlb16M {
    fn from(value: Tlb) -> Self {
        Self::new().with_local_offset(value.local_offset as u64)
            .with_x_end(value.x_end as u64)
            .with_y_end(value.y_end as u64)
            .with_x_start(value.x_start as u64)
            .with_y_start(value.y_start as u64)
            .with_noc_sel(value.noc_sel)
            .with_mcast(value.mcast)
            .with_ordering(value.ordering as u64)
            .with_linked(value.linked)
    }
}

// For WH we have 156 1MB TLBS, 10 2MB TLBS and 20 16 MB TLBs
// For now I'll allow all to be programmed, but I'll only use tlb 20
pub fn setup_tlb(chip: &mut Chip, tlb_index: u32, mut tlb: Tlb) -> Result<(u32, usize), PciError> {
    const TLB_CONFIG_BASE: u64 = 0x1FC00000;

    const TLB_COUNT_1M: u64 = 156;
    const TLB_COUNT_2M: u64 = 10;
    const TLB_COUNT_16M: u64 = 20;

    const TLB_INDEX_1M: u64 = 0;
    const TLB_INDEX_2M: u64 = TLB_COUNT_1M;
    const TLB_INDEX_16M: u64 = TLB_COUNT_1M + TLB_COUNT_2M;

    const TLB_BASE_1M: u64 = 0;
    const TLB_BASE_2M: u64 = TLB_COUNT_1M * (1 << 20);
    const TLB_BASE_16M: u64 = TLB_BASE_2M + TLB_COUNT_2M * (1 << 21);

    let tlb_config_addr = TLB_CONFIG_BASE + (tlb_index as u64 * 8);

    let (tlb_value, mmio_addr, size, addr_offset) = match tlb_index {
        0..=155 => {
            let size = 1 << 20;
            let tlb_address = tlb.local_offset as u64 / size;
            let local_offset = tlb.local_offset % size;

            tlb.local_offset = tlb_address;
            (Tlb1M::from(tlb).0, TLB_BASE_1M + size * tlb_index as u64, size, local_offset)
        }
        156..=165 => {
            let size = 1 << 21;
            let tlb_address = tlb.local_offset as u64 / size;
            let local_offset = tlb.local_offset % size;

            tlb.local_offset = tlb_address;
            (Tlb2M::from(tlb).0, TLB_BASE_2M + size * (tlb_index - 156) as u64, size, local_offset)
        }
        166..=185 => {
            let size = 1 << 24;
            let tlb_address = tlb.local_offset as u64 / size;
            let local_offset = tlb.local_offset % size;

            tlb.local_offset = tlb_address;
            (Tlb16M::from(tlb).0, TLB_BASE_16M + size * (tlb_index - 166) as u64, size, local_offset)
        }
        _ => {
            panic!("TLB index out of range");
        }
    };

    chip.transport.write32(tlb_config_addr as u32, (tlb_value & 0xFFFF_FFFF) as u32)?;
    chip.transport.write32(tlb_config_addr as u32 + 4, ((tlb_value >> 32) & 0xFFFF_FFFF) as u32)?;

    Ok((mmio_addr as u32 + addr_offset as u32, size as usize - addr_offset as usize))
}
