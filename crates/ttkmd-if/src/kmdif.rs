// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

pub const MAX_DMA_BYTES: u32 = 4 * 1024 * 1024;
pub const GS_BAR0_WC_MAPPING_SIZE: u64 = (156 << 20) + (10 << 21) + (18 << 24);
pub const BH_BAR0_WC_MAPPING_SIZE: u64 = 188 << 21;

pub const GS_WH_ARC_SCRATCH6_ADDR: u32 = 0x1ff30078;
pub const BH_NOC_NODE_ID_OFFSET: u32 = 0x1FD04044;

#[repr(u32)]
#[derive(Debug, Clone, Copy)]
pub enum MappingId {
    Unused = 0,
    Resource0Uc = 1,
    Resource0Wc = 2,
    Resource1Uc = 3,
    Resource1Wc = 4,
    Resource2Uc = 5,
    Resource2Wc = 6,
    Unknown(u32),
}

impl MappingId {
    pub fn from_u32(value: u32) -> MappingId {
        match value {
            0 => MappingId::Unused,
            1 => MappingId::Resource0Uc,
            2 => MappingId::Resource0Wc,
            3 => MappingId::Resource1Uc,
            4 => MappingId::Resource1Wc,
            5 => MappingId::Resource2Uc,
            6 => MappingId::Resource2Wc,
            v => MappingId::Unknown(v),
        }
    }

    pub fn as_u32(&self) -> u32 {
        // SAFTEY: Need to ensure that the enum has a primitive representation for this to be defined
        unsafe { *(self as *const Self as *const u32) }
    }
}

pub fn getpagesize() -> Option<i64> {
    nix::unistd::sysconf(nix::unistd::SysconfVar::PAGE_SIZE)
        .ok()
        .flatten()
}

#[bitfield_struct::bitfield(u32)]
pub struct DmaPack {
    #[bits(28)]
    pub size_bytes: u32, // Transfer size in bytes
    pub write: bool,              // 0 = Chip -> Host, 1 = Host -> Chip
    pub pcie_msi_on_done: bool, // Whether to configure DMA engine to send MSI on completion. pcie_msi_on_done and pcie_write_on_done are exclusive.
    pub pcie_write_on_done: bool, // Instead of triggering an MSI, write to a location stored in pcie_config_t.completion_flag_phys_addr. pcie_msi_on_done and pcie_write_on_done are exclusive.
    pub trigger: bool,            // 1 = Start transfer. The handler should reset it to 0.
}

#[repr(C)]
pub struct ArcPcieCtrlDmaRequest {
    pub chip_addr: u32,                 // Local address (on the device)
    pub host_phys_addr_lo: u32,         // Host physical address (this is physical address)
    pub completion_flag_phys_addr: u32, // Pointer to the completion flag - the dma engine will write to this address to report completion
    pub dma_pack: DmaPack,
    pub repeat: u32, // How many times to repeat the oparation (for debug only) bit31 indicates whether the request is 64 bit transfer
} // 5 * 4 = 20B
