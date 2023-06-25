mod tlb;

use kmdif::{DmaBuffer, PciError, PciDevice};

use crate::{
    common::{ArcMsg, Chip},
    remote::{self, detect, EthCoord, IntoChip, RemoteWormholeChip},
    TTError, axi::{Axi, AxiReadWrite},
};

pub struct Noc<'a> {
    noc_id: bool,
    chip: &'a mut Chip,
}

const TLB_INDEX: u32 = 170;

impl Noc<'_> {
    pub fn read32(&mut self, x: u8, y: u8, addr: u32) -> Result<u32, PciError> {
        let (bar_addr, _slice_len) = tlb::setup_tlb(
            &mut self.chip,
            TLB_INDEX,
            tlb::Tlb {
                local_offset: addr as u64,
                x_end: x,
                y_end: y,
                noc_sel: self.noc_id,
                mcast: false,
                ..Default::default()
            },
        )?;

        self.chip.transport.read32(bar_addr)
    }

    pub fn block_read(&mut self, x: u8, y: u8, addr: u32, data: &mut [u8]) -> Result<(), PciError> {
        let mut offset = 0;
        let mut to_read = data;
        while !to_read.is_empty() {
            let (bar_addr, slice_len) = tlb::setup_tlb(
                &mut self.chip,
                TLB_INDEX,
                tlb::Tlb {
                    local_offset: addr as u64 + offset,
                    x_end: x,
                    y_end: y,
                    noc_sel: self.noc_id,
                    mcast: false,
                    ..Default::default()
                },
            )?;

            let (data, rest) = if to_read.len() > slice_len {
                to_read.split_at_mut(slice_len as usize)
            } else {
                let empty = unsafe { std::slice::from_raw_parts_mut(to_read.as_mut_ptr(), 0) };
                (to_read, empty)
            };

            self.chip.transport.read_block(bar_addr, data)?;

            to_read = rest;
            offset += slice_len as u64;
        }

        Ok(())
    }

    pub fn write32(&mut self, x: u8, y: u8, addr: u32, value: u32) -> Result<(), PciError> {
        let (bar_addr, _slice_len) = tlb::setup_tlb(
            &mut self.chip,
            TLB_INDEX,
            tlb::Tlb {
                local_offset: addr as u64,
                x_end: x,
                y_end: y,
                noc_sel: self.noc_id,
                mcast: false,
                ..Default::default()
            },
        )?;

        self.chip.transport.write32(bar_addr, value)?;

        Ok(())
    }

    pub fn block_write(&mut self, x: u8, y: u8, addr: u32, data: &[u8]) -> Result<(), PciError> {
        let mut offset = 0;
        let mut to_write = data;
        while !to_write.is_empty() {
            let (bar_addr, slice_len) = tlb::setup_tlb(
                &mut self.chip,
                TLB_INDEX,
                tlb::Tlb {
                    local_offset: addr as u64 + offset as u64,
                    x_end: x,
                    y_end: y,
                    noc_sel: self.noc_id,
                    mcast: false,
                    ..Default::default()
                },
            )?;

            let (data, rest) = if to_write.len() > slice_len {
                to_write.split_at(slice_len as usize)
            } else {
                (to_write, &to_write[0..0])
            };

            self.chip.transport.write_block(bar_addr, data)?;

            offset += slice_len;
            to_write = rest;
        }

        Ok(())
    }
}

pub struct Wormhole {
    pub chip: Chip,

    eth_dma: Option<DmaBuffer>,
}

impl Wormhole {
    pub fn create(device_id: usize) -> Result<Self, TTError> {
        let mut chip = Chip::create(device_id)?;
        chip.axi = Axi::new("wormhole-axi-pci.bin");

        Self::new(chip)
    }

    pub fn new(mut chip: Chip) -> Result<Self, TTError> {
        if let kmdif::Arch::Wormhole = chip.arch() {
            chip.axi = Axi::new("wormhole-axi-pci.bin");
            Ok(Self {
                chip,
                eth_dma: None,
            })
        } else {
            Err(TTError::ArchMismatch {
                expected: kmdif::Arch::Wormhole,
                actual: chip.arch(),
            })
        }
    }

    pub fn axi(&mut self) -> AxiReadWrite {
        self.chip.axi()
    }

    pub fn board_id(&mut self) -> Result<u64, TTError> {
        let lower = self.axi().read::<u32>("ARC_CSM.BOARD_INFO[0]")?;
        let upper = self.axi().read::<u32>("ARC_CSM.BOARD_INFO[1]")?;

        Ok((upper as u64) << 32 | lower as u64)
        // Ok(
        //     ((self.chip.transport.read32(0x1FE80000 + 0x78828 + 0x10C)? as u64) << 32)
        //         | self.chip.transport.read32(0x1FE80000 + 0x78828 + 0x108)? as u64,
        // )
    }

    pub fn coord(&mut self) -> Result<EthCoord, PciError> {
        remote::get_local_chip_coord(self)
    }

    pub fn remote(
        &mut self,
        coord: impl IntoChip<EthCoord>,
    ) -> Result<RemoteWormholeChip, PciError> {
        let coord = coord.cinto(self)?;
        RemoteWormholeChip::create(self, 9, 0, false, std::time::Duration::from_secs(11), coord)
    }

    pub fn noc(&mut self, id: bool) -> Noc {
        Noc {
            noc_id: id,
            chip: &mut self.chip,
        }
    }

    pub fn arc_msg(
        &mut self,
        msg: &mut ArcMsg,
        wait_for_done: bool,
        timeout: std::time::Duration,
        use_second_mailbox: bool,
    ) -> Result<crate::common::ArcMsgOk, crate::common::ArcMsgError> {
        self.chip
            .arc_msg(msg, wait_for_done, timeout, use_second_mailbox)
    }

    pub fn get_eth_dma_buffer(&mut self) -> Result<&mut DmaBuffer, PciError> {
        if self.eth_dma.is_none() {
            self.eth_dma = Some(self.chip.transport.allocate_dma_buffer(1 << 20)?);
        }
        Ok(self.eth_dma.as_mut().unwrap())
    }
}
