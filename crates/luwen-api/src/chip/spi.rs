// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{AxiError, HlCommsInterface};
use crate::{arc_msg::TypedArcMsg, ArcMsg, ChipImpl};

pub struct Spi {
    gpio2_pad_trien_cntl: u64,
    gpio2_pad_drv_cntl: u64,
    gpio2_pad_rxen_cntl: u64,

    spi_cntl: u64,

    spi_ctrlr0: u64,
    spi_ctrlr1: u64,
    spi_ssienr: u64,
    spi_ser: u64,
    spi_sr: u64,
    spi_dr: u64,
    spi_baudr: u64,
}

const RX_BUFFER_DEPTH: u32 = 8;
const TX_BUFFER_DEPTH: u32 = 8;
const PAGE_SIZE: u32 = 256;
const SECTOR_SIZE: u32 = 4 * 1024;

const SPI_CNTL_CLK_DISABLE: u32 = 0x1 << 8;
const SPI_CNTL_SPI_DISABLE: u32 = 0x0;
const SPI_CNTL_SPI_ENABLE: u32 = 0x1;

const SPI_SSIENR_ENABLE: u32 = 0x1;
const SPI_SSIENR_DISABLE: u32 = 0x0;

const SPI_WR_EN_CMD: u8 = 0x06;
const SPI_WR_ER_CMD: u8 = 0x20;
const SPI_RD_CMD: u8 = 0x03;
const SPI_WR_CMD: u8 = 0x02;
const SPI_RD_STATUS_CMD: u8 = 0x05;
const SPI_WR_STATUS_CMD: u8 = 0x01;
const SPI_CHIP_ERASE_CMD: u8 = 0xC7;

fn spi_ctrl0_spi_scph(scph: u32) -> u32 {
    (scph << 6) & 0x1
}

const SPI_CTRL0_TMOD_TRANSMIT_ONLY: u32 = 0x1 << 8;
const SPI_CTRL0_TMOD_EEPROM_READ: u32 = 0x3 << 8;
const SPI_CTRL0_SPI_FRF_STANDARD: u32 = 0x0 << 21;
const SPI_CTRL0_DFS32_FRAME_08BITS: u32 = 0x7 << 16;

fn spi_ctrl1_ndf(frame_count: u32) -> u32 {
    frame_count & 0xffff
}

const SPI_SR_RFNE: u32 = 0x1 << 3;
const SPI_SR_TFE: u32 = 0x1 << 2;
const SPI_SR_BUSY: u32 = 0x1 << 0;

const SPI_PAGE_ERASE_SIZE: u32 = 0x1000;
const SPI_ROM_SIZE: u32 = 1 << 24;
const ARC_SPI_CHUNK_SIZE: u32 = SPI_PAGE_ERASE_SIZE;

fn spi_ser_slave_disable(slave_id: u32) -> u32 {
    0x0 << slave_id
}

fn spi_ser_slave_enable(slave_id: u32) -> u32 {
    0x1 << slave_id
}

fn spi_baudr_sckdv(ssi_clk_div: u32) -> u32 {
    ssi_clk_div & 0xffff
}

impl Spi {
    pub fn new(chip: &impl ChipImpl) -> Result<Self, AxiError> {
        Ok(Spi {
            gpio2_pad_trien_cntl: chip.axi_translate("ARC_RESET.GPIO2_PAD_TRIEN_CNTL")?.addr,
            gpio2_pad_drv_cntl: chip.axi_translate("ARC_RESET.GPIO2_PAD_DRV_CNTL")?.addr,
            gpio2_pad_rxen_cntl: chip.axi_translate("ARC_RESET.GPIO2_PAD_RXEN_CNTL")?.addr,

            spi_ctrlr0: chip.axi_translate("ARC_SPI.SPI_CTRLR0")?.addr,
            spi_ctrlr1: chip.axi_translate("ARC_SPI.SPI_CTRLR1")?.addr,
            spi_ssienr: chip.axi_translate("ARC_SPI.SPI_SSIENR")?.addr,
            spi_ser: chip.axi_translate("ARC_SPI.SPI_SER")?.addr,
            spi_sr: chip.axi_translate("ARC_SPI.SPI_SR")?.addr,
            spi_dr: chip.axi_translate("ARC_SPI.SPI_DR")?.addr,
            spi_baudr: chip.axi_translate("ARC_SPI.SPI_BAUDR")?.addr,

            spi_cntl: chip.axi_translate("ARC_RESET.SPI_CNTL")?.addr,
        })
    }

    fn init(&self, chip: &impl ChipImpl, clock_div: u32) -> Result<(), Box<dyn std::error::Error>> {
        let mut reg = chip.axi_read32(self.gpio2_pad_trien_cntl)?;
        reg |= 1 << 2; // Enable tristate for SPI data in PAD
        reg &= !(1 << 5); // Disable tristate for SPI chip select PAD
        reg &= !(1 << 6); // Disable tristate for SPI clock PAD
        chip.axi_write32(self.gpio2_pad_trien_cntl, reg)?;

        chip.axi_write32(self.gpio2_pad_drv_cntl, 0xffffffff)?;

        // Enable RX for all SPI PADS
        let mut reg = chip.axi_read32(self.gpio2_pad_rxen_cntl)?;
        reg |= 0x3f << 1; // PADs 1 to 6 are used for SPI quad SCPH support
        chip.axi_write32(self.gpio2_pad_rxen_cntl, reg)?;
        chip.axi_write32(self.spi_cntl, SPI_CNTL_SPI_ENABLE)?;

        chip.axi_write32(self.spi_ssienr, SPI_SSIENR_DISABLE)?;
        chip.axi_write32(
            self.spi_ctrlr0,
            SPI_CTRL0_TMOD_EEPROM_READ
                | SPI_CTRL0_SPI_FRF_STANDARD
                | SPI_CTRL0_DFS32_FRAME_08BITS
                | spi_ctrl0_spi_scph(0x1),
        )?;
        chip.axi_write32(self.spi_ser, 0)?;
        chip.axi_write32(self.spi_baudr, spi_baudr_sckdv(clock_div))?;
        chip.axi_write32(self.spi_ssienr, SPI_SSIENR_ENABLE)?;

        Ok(())
    }

    fn disable(&self, chip: &impl ChipImpl) -> Result<(), Box<dyn std::error::Error>> {
        chip.axi_write32(self.spi_cntl, SPI_CNTL_CLK_DISABLE | SPI_CNTL_SPI_DISABLE)?;

        Ok(())
    }

    pub fn read_status(
        &self,
        chip: &impl ChipImpl,
        register: u8,
    ) -> Result<u8, Box<dyn std::error::Error>> {
        chip.axi_write32(self.spi_ssienr, SPI_SSIENR_DISABLE)?;
        chip.axi_write32(
            self.spi_ctrlr0,
            SPI_CTRL0_TMOD_EEPROM_READ
                | SPI_CTRL0_SPI_FRF_STANDARD
                | SPI_CTRL0_DFS32_FRAME_08BITS
                | spi_ctrl0_spi_scph(0x1),
        )?;
        chip.axi_write32(self.spi_ctrlr1, spi_ctrl1_ndf(0))?;
        chip.axi_write32(self.spi_ssienr, SPI_SSIENR_ENABLE)?;

        chip.axi_write32(self.spi_ser, spi_ser_slave_disable(0))?;

        // Write status register to read
        chip.axi_write32(self.spi_dr, register as u32)?;
        chip.axi_write32(self.spi_ser, spi_ser_slave_enable(0))?;

        // Read value
        loop {
            if (chip.axi_read32(self.spi_sr)? & SPI_SR_RFNE) != 0 {
                break;
            }
        }
        let read_buf = (chip.axi_read32(self.spi_dr)? & 0xff) as u8;

        chip.axi_write32(self.spi_ser, spi_ser_slave_disable(0))?;

        Ok(read_buf)
    }

    pub fn unlock(&self, chip: &impl ChipImpl) -> Result<(), Box<dyn std::error::Error>> {
        self.lock(chip, 0)
    }

    pub fn lock(
        &self,
        chip: &impl ChipImpl,
        sections: u8,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Set slave address
        chip.axi_write32(self.spi_ssienr, SPI_SSIENR_DISABLE)?;
        chip.axi_write32(
            self.spi_ctrlr0,
            SPI_CTRL0_TMOD_TRANSMIT_ONLY
                | SPI_CTRL0_SPI_FRF_STANDARD
                | SPI_CTRL0_DFS32_FRAME_08BITS
                | spi_ctrl0_spi_scph(0x1),
        )?;
        chip.axi_write32(self.spi_ssienr, SPI_SSIENR_ENABLE)?;
        chip.axi_write32(self.spi_ser, spi_ser_slave_disable(0))?;

        // Enable write
        chip.axi_write32(self.spi_dr, SPI_WR_EN_CMD as u32)?;
        chip.axi_write32(self.spi_ser, spi_ser_slave_enable(0))?;

        // Add some delay to make sure the above propagates
        loop {
            let spi_status = chip.axi_read32(self.spi_sr)?;
            if spi_status & SPI_SR_TFE == SPI_SR_TFE {
                break;
            }
        }
        loop {
            let spi_status = chip.axi_read32(self.spi_sr)?;
            if spi_status & SPI_SR_BUSY != SPI_SR_BUSY {
                break;
            }
        }

        chip.axi_write32(self.spi_ser, spi_ser_slave_disable(0))?;

        // Write sectors to lock
        chip.axi_write32(self.spi_dr, SPI_WR_STATUS_CMD as u32)?;

        // Figure out which SPI to use
        let simple_spi = if let Ok(Some(info)) = chip.get_device_info() {
            info.board_id == 0x35
        } else {
            let telem = chip.get_telemetry()?;
            let upi = (telem.board_id >> (32 + 4)) & 0xFFFFF;
            upi == 0x35
        };

        // Write sector lock info
        if simple_spi {
            chip.axi_write32(self.spi_dr, (1 << 6) | ((sections as u32) << 2))?;
        } else if sections < 5 {
            chip.axi_write32(self.spi_dr, (0x3 << 5) | ((sections as u32) << 2))?;
        } else {
            chip.axi_write32(self.spi_dr, (0x1 << 5) | ((sections as u32 - 5) << 2))?;
        }
        chip.axi_write32(self.spi_ser, spi_ser_slave_enable(0))?;

        // Add some delay to make sure the above propagates
        loop {
            let spi_status = chip.axi_read32(self.spi_sr)?;
            if spi_status & SPI_SR_TFE == SPI_SR_TFE {
                break;
            }
        }
        loop {
            let spi_status = chip.axi_read32(self.spi_sr)?;
            if spi_status & SPI_SR_BUSY != SPI_SR_BUSY {
                break;
            }
        }

        chip.axi_write32(self.spi_ser, spi_ser_slave_disable(0))?;

        // Wait for lock operation to complete
        loop {
            let busy = self.read_status(chip, SPI_RD_STATUS_CMD)? & 0x1;
            if busy != 0x1 {
                break;
            }
        }

        Ok(())
    }

    pub fn read(
        &self,
        chip: &impl ChipImpl,
        mut addr: u32,
        read: &mut [u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        chip.axi_write32(self.spi_ssienr, SPI_SSIENR_DISABLE)?;
        chip.axi_write32(
            self.spi_ctrlr0,
            SPI_CTRL0_TMOD_EEPROM_READ
                | SPI_CTRL0_SPI_FRF_STANDARD
                | SPI_CTRL0_DFS32_FRAME_08BITS
                | spi_ctrl0_spi_scph(0x1),
        )?;
        chip.axi_write32(self.spi_ssienr, SPI_SSIENR_ENABLE)?;

        let mut frame_index = 0;
        while frame_index < read.len() {
            let frames = RX_BUFFER_DEPTH.min((read.len() - frame_index) as u32);

            // Write slave addr
            chip.axi_write32(self.spi_ssienr, SPI_SSIENR_DISABLE)?;

            // Frames to read
            chip.axi_write32(self.spi_ctrlr1, spi_ctrl1_ndf(frames - 1))?;

            chip.axi_write32(self.spi_ssienr, SPI_SSIENR_ENABLE)?;
            chip.axi_write32(self.spi_ser, spi_ser_slave_disable(0))?;

            // Write address to read
            chip.axi_write32(self.spi_dr, SPI_RD_CMD as u32)?;
            chip.axi_write32(self.spi_dr, (addr >> 16) & 0xff)?;
            chip.axi_write32(self.spi_dr, (addr >> 8) & 0xff)?;
            chip.axi_write32(self.spi_dr, addr & 0xff)?;

            chip.axi_write32(self.spi_ser, spi_ser_slave_enable(0))?;

            // read frames
            for _ in 0..frames {
                loop {
                    let spi_status = chip.axi_read32(self.spi_sr)?;
                    if spi_status & SPI_SR_RFNE != 0 {
                        break;
                    }
                }

                read[frame_index] = (chip.axi_read32(self.spi_dr)? & 0xff) as u8;

                frame_index += 1;
            }

            addr += frames;

            chip.axi_write32(self.spi_ser, spi_ser_slave_disable(0))?;
        }

        Ok(())
    }

    fn raw_write(
        &self,
        chip: &impl ChipImpl,
        mut addr: u32,
        write: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut frame_index = 0;
        while frame_index < write.len() {
            let frames = (TX_BUFFER_DEPTH - 4)
                .min((write.len() - frame_index) as u32)
                .min(PAGE_SIZE - (addr % PAGE_SIZE));

            chip.axi_write32(self.spi_ssienr, SPI_SSIENR_DISABLE)?;
            chip.axi_write32(
                self.spi_ctrlr0,
                SPI_CTRL0_TMOD_TRANSMIT_ONLY
                    | SPI_CTRL0_SPI_FRF_STANDARD
                    | SPI_CTRL0_DFS32_FRAME_08BITS
                    | spi_ctrl0_spi_scph(0x1),
            )?;
            chip.axi_write32(self.spi_ssienr, SPI_SSIENR_ENABLE)?;
            chip.axi_write32(self.spi_ser, spi_ser_slave_disable(0))?;

            // Write enable
            chip.axi_write32(self.spi_dr, SPI_WR_EN_CMD as u32)?;
            chip.axi_write32(self.spi_ser, spi_ser_slave_enable(0))?;

            // Add some delay to make sure enable propagates
            loop {
                let spi_status = chip.axi_read32(self.spi_sr)?;
                if spi_status & SPI_SR_TFE == SPI_SR_TFE {
                    break;
                }
            }
            loop {
                let spi_status = chip.axi_read32(self.spi_sr)?;
                if spi_status & SPI_SR_BUSY != SPI_SR_BUSY {
                    break;
                }
            }
            chip.axi_write32(self.spi_ser, spi_ser_slave_disable(0))?;

            // Write address and values
            chip.axi_write32(self.spi_dr, SPI_WR_CMD as u32)?;
            chip.axi_write32(self.spi_dr, (addr >> 16) & 0xff)?;
            chip.axi_write32(self.spi_dr, (addr >> 8) & 0xff)?;
            chip.axi_write32(self.spi_dr, addr & 0xff)?;
            for _ in 0..frames {
                chip.axi_write32(self.spi_dr, write[frame_index] as u32)?;
                frame_index += 1;
            }

            chip.axi_write32(self.spi_ser, spi_ser_slave_enable(0))?;

            // Add some delay to make sure that the enable above propogates
            loop {
                let spi_status = chip.axi_read32(self.spi_sr)?;
                if spi_status & SPI_SR_TFE == SPI_SR_TFE {
                    break;
                }
            }
            loop {
                let spi_status = chip.axi_read32(self.spi_sr)?;
                if spi_status & SPI_SR_BUSY != SPI_SR_BUSY {
                    break;
                }
            }

            chip.axi_write32(self.spi_ser, spi_ser_slave_disable(0))?;
            addr += frames;

            // Wait for write to complete
            loop {
                let busy = self.read_status(chip, SPI_RD_STATUS_CMD)? & 0x1;
                if busy != 0x1 {
                    break;
                }
            }
        }

        Ok(())
    }

    fn erase(&self, chip: &impl ChipImpl, addr: u32) -> Result<(), Box<dyn std::error::Error>> {
        chip.axi_write32(self.spi_ssienr, SPI_SSIENR_DISABLE)?;
        chip.axi_write32(
            self.spi_ctrlr0,
            SPI_CTRL0_TMOD_TRANSMIT_ONLY
                | SPI_CTRL0_SPI_FRF_STANDARD
                | SPI_CTRL0_DFS32_FRAME_08BITS
                | spi_ctrl0_spi_scph(0x1),
        )?;
        chip.axi_write32(self.spi_ssienr, SPI_SSIENR_ENABLE)?;
        chip.axi_write32(self.spi_ser, spi_ser_slave_disable(0))?;

        // Write enable
        chip.axi_write32(self.spi_dr, SPI_WR_EN_CMD as u32)?;
        chip.axi_write32(self.spi_ser, spi_ser_slave_enable(0))?;

        // Add some delay to make sure enable propagates
        loop {
            let spi_status = chip.axi_read32(self.spi_sr)?;
            if spi_status & SPI_SR_TFE == SPI_SR_TFE {
                break;
            }
        }
        loop {
            let spi_status = chip.axi_read32(self.spi_sr)?;
            if spi_status & SPI_SR_BUSY != SPI_SR_BUSY {
                break;
            }
        }
        chip.axi_write32(self.spi_ser, spi_ser_slave_disable(0))?;

        // Write sector to erase
        chip.axi_write32(self.spi_dr, SPI_WR_ER_CMD as u32)?;
        chip.axi_write32(self.spi_dr, (addr >> 16) & 0xff)?;
        chip.axi_write32(self.spi_dr, (addr >> 8) & 0xff)?;
        chip.axi_write32(self.spi_dr, addr & 0xff)?;
        chip.axi_write32(self.spi_ser, spi_ser_slave_enable(0))?;

        // Add some delay to make sure enable propagates
        loop {
            let spi_status = chip.axi_read32(self.spi_sr)?;
            if spi_status & SPI_SR_TFE == SPI_SR_TFE {
                break;
            }
        }
        loop {
            let spi_status = chip.axi_read32(self.spi_sr)?;
            if spi_status & SPI_SR_BUSY != SPI_SR_BUSY {
                break;
            }
        }
        chip.axi_write32(self.spi_ser, spi_ser_slave_disable(0))?;

        // Wait for erase to complete
        loop {
            let busy = self.read_status(chip, SPI_RD_STATUS_CMD)? & 0x1;
            if busy != 0x1 {
                break;
            }
        }

        Ok(())
    }

    #[allow(dead_code)]
    fn full_erase(&self, chip: &impl ChipImpl) -> Result<(), Box<dyn std::error::Error>> {
        chip.axi_write32(self.spi_ssienr, SPI_SSIENR_DISABLE)?;
        chip.axi_write32(
            self.spi_ctrlr0,
            SPI_CTRL0_TMOD_TRANSMIT_ONLY
                | SPI_CTRL0_SPI_FRF_STANDARD
                | SPI_CTRL0_DFS32_FRAME_08BITS
                | spi_ctrl0_spi_scph(0x1),
        )?;
        chip.axi_write32(self.spi_ssienr, SPI_SSIENR_ENABLE)?;
        chip.axi_write32(self.spi_ser, spi_ser_slave_disable(0))?;

        // Write enable
        chip.axi_write32(self.spi_dr, SPI_WR_EN_CMD as u32)?;
        chip.axi_write32(self.spi_ser, spi_ser_slave_enable(0))?;

        // Add some delay to make sure enable propagates
        loop {
            let spi_status = chip.axi_read32(self.spi_sr)?;
            if spi_status & SPI_SR_TFE == SPI_SR_TFE {
                break;
            }
        }
        loop {
            let spi_status = chip.axi_read32(self.spi_sr)?;
            if spi_status & SPI_SR_BUSY != SPI_SR_BUSY {
                break;
            }
        }
        chip.axi_write32(self.spi_ser, spi_ser_slave_disable(0))?;

        // Write chip erase command
        chip.axi_write32(self.spi_dr, SPI_CHIP_ERASE_CMD as u32)?;
        chip.axi_write32(self.spi_ser, spi_ser_slave_enable(0))?;

        // Add some delay to make sure enable propagates
        loop {
            let spi_status = chip.axi_read32(self.spi_sr)?;
            if spi_status & SPI_SR_TFE == SPI_SR_TFE {
                break;
            }
        }
        loop {
            let spi_status = chip.axi_read32(self.spi_sr)?;
            if spi_status & SPI_SR_BUSY != SPI_SR_BUSY {
                break;
            }
        }
        chip.axi_write32(self.spi_ser, spi_ser_slave_disable(0))?;

        // Wait for erase to complete
        loop {
            let busy = self.read_status(chip, SPI_RD_STATUS_CMD)? & 0x1;
            if busy != 0x1 {
                break;
            }
        }

        Ok(())
    }

    pub fn write(
        &self,
        chip: &impl ChipImpl,
        addr: u32,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut existing = vec![0; data.len()];
        self.read(chip, addr, &mut existing)?;

        if existing.iter().copied().all(|v| v == 0xff) {
            self.raw_write(chip, addr, data)?;
        } else if existing
            .iter()
            .copied()
            .zip(data.iter().copied())
            .any(|(a, b)| a != b)
        {
            let sector_start = addr / SECTOR_SIZE;
            let num_sectors = (addr + data.len() as u32).div_ceil(SECTOR_SIZE) - sector_start;

            existing.resize((num_sectors * SECTOR_SIZE) as usize, 0);
            self.read(chip, sector_start * SECTOR_SIZE, &mut existing)?;

            let existing_offset = addr - (sector_start * SECTOR_SIZE);

            for (i, o) in existing[existing_offset as usize..][..data.len()]
                .iter_mut()
                .zip(data.iter().copied())
            {
                *i = o;
            }

            for i in 0..num_sectors {
                let sector_address = (sector_start + i) * SECTOR_SIZE;
                let sector_offset = i * SECTOR_SIZE;

                self.erase(chip, sector_address)?;
                self.raw_write(
                    chip,
                    sector_address,
                    &existing[sector_offset as usize..][..SECTOR_SIZE as usize],
                )?;
            }
        }

        Ok(())
    }
}

pub struct ActiveSpi {
    spi: Spi,
    use_arc: bool,
}

impl ActiveSpi {
    pub fn new(chip: &impl ChipImpl, use_arc: bool) -> Result<Self, Box<dyn std::error::Error>> {
        let spi = Spi::new(chip)?;

        Ok(ActiveSpi { spi, use_arc })
    }

    fn spi_arc_read_chunk(
        chip: &impl ChipImpl,
        spi_dump_addr: u64,
        addr: u32,
    ) -> Result<[u8; ARC_SPI_CHUNK_SIZE as usize], Box<dyn std::error::Error>> {
        chip.arc_msg(super::ArcMsgOptions {
            msg: ArcMsg::Typed(TypedArcMsg::SpiRead { addr }),
            ..Default::default()
        })?;

        let mut data = [0; ARC_SPI_CHUNK_SIZE as usize];
        chip.axi_read(spi_dump_addr, &mut data)?;

        Ok(data)
    }

    /// This will reuse the last spi address that we read from.
    /// Callers will need to ensure that we successfully read from the address that we want to read
    /// from.
    fn spi_arc_write_chunk(
        chip: &impl ChipImpl,
        spi_dump_addr: u64,
        data: [u8; ARC_SPI_CHUNK_SIZE as usize],
    ) -> Result<(), Box<dyn std::error::Error>> {
        chip.axi_write(spi_dump_addr, &data)?;
        chip.arc_msg(super::ArcMsgOptions {
            msg: ArcMsg::Typed(TypedArcMsg::SpiWrite),
            ..Default::default()
        })?;

        Ok(())
    }

    fn get_aligned_params(addr: u32, size: u32) -> Result<(u32, u32, u32), String> {
        if addr + size > SPI_ROM_SIZE {
            return Err("Requested size exceeds SPI ROM.".to_string());
        }

        let start_addr = (addr / ARC_SPI_CHUNK_SIZE) * ARC_SPI_CHUNK_SIZE; // round down
        let end_addr = (addr + size).div_ceil(ARC_SPI_CHUNK_SIZE) * ARC_SPI_CHUNK_SIZE;
        let num_chunks = (end_addr - start_addr) / ARC_SPI_CHUNK_SIZE;
        let start_offset = addr - start_addr;

        Ok((start_addr, num_chunks, start_offset))
    }

    fn get_spi_dump_address(
        chip: &impl ChipImpl,
    ) -> Result<Option<u64>, Box<dyn std::error::Error>> {
        let dump_addr = if let Ok(result) = chip.arc_msg(super::ArcMsgOptions {
            msg: ArcMsg::Typed(TypedArcMsg::GetSpiDumpAddr),
            ..Default::default()
        }) {
            match result {
                crate::ArcMsgOk::Ok { rc: _, arg } => Some(arg),
                crate::ArcMsgOk::OkNoWait => None,
            }
        } else {
            None
        };

        let csm_offset = chip.axi_translate("ARC_CSM.DATA[0]")?.addr - 0x10000000_u64;

        if let Some(dump_addr) = dump_addr {
            Ok(Some(csm_offset + (dump_addr as u64)))
        } else {
            Ok(None)
        }
    }

    fn spi_arc_read(
        &self,
        chip: &impl ChipImpl,
        spi_dump_addr: u64,
        addr: u32,
        data: &mut [u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (start_addr, num_chunks, start_offset) =
            Self::get_aligned_params(addr, data.len() as u32)?;

        for chunk in 0..num_chunks {
            let offset = chunk * ARC_SPI_CHUNK_SIZE;
            let addr = start_addr + offset;

            let read_data = Self::spi_arc_read_chunk(chip, spi_dump_addr, addr)?;

            if offset < start_offset {
                for (a, b) in data[offset as usize..]
                    .iter_mut()
                    .zip(read_data[(start_offset - offset) as usize..].iter())
                {
                    *a = *b;
                }
            } else {
                for (a, b) in data[(offset - start_offset) as usize..]
                    .iter_mut()
                    .zip(read_data.into_iter())
                {
                    *a = b;
                }
            };
        }

        Ok(())
    }

    fn spi_arc_write(
        &self,
        chip: &impl ChipImpl,
        spi_dump_addr: u64,
        addr: u32,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (start_addr, num_chunks, start_offset) =
            Self::get_aligned_params(addr, data.len() as u32)?;

        for chunk in 0..num_chunks {
            let offset = chunk * ARC_SPI_CHUNK_SIZE;
            let addr = start_addr + offset;

            let mut read_data = Self::spi_arc_read_chunk(chip, spi_dump_addr, addr)?;
            let orig_data = read_data;
            if offset < start_offset {
                for (a, b) in data[offset as usize..]
                    .iter()
                    .zip(read_data[(start_offset - offset) as usize..].iter_mut())
                {
                    *b = *a;
                }
            } else {
                for (a, b) in data[(offset - start_offset) as usize..]
                    .iter()
                    .zip(read_data.iter_mut())
                {
                    *b = *a;
                }
            };

            if read_data != orig_data {
                Self::spi_arc_write_chunk(chip, spi_dump_addr, read_data)?;
            }
        }

        Ok(())
    }

    fn get_clock(&self, chip: &impl ChipImpl) -> Result<u32, Box<dyn std::error::Error>> {
        let arcclk = if let Ok(telemetry) = chip.get_telemetry() {
            telemetry.arcclk
        } else {
            // If teletry failed then we are pessemistic and assume 540 MHz
            540
        };

        let mut clock_div = (arcclk as f32 / 20.0).ceil() as u32;
        clock_div += clock_div % 2;

        Ok(clock_div)
    }

    /// Write to the spi.
    /// Unfortunatly writing to the eeprom on the other end of the spi bus is a fairly delicate operation.
    /// In general we can only write to an erased section, also the
    /// will unlock and then relock the spi. Will
    pub fn write(
        &self,
        chip: &impl ChipImpl,
        addr: u32,
        write: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let clock_div = self.get_clock(chip)?;

        // Must call init before unlock
        self.spi.init(chip, clock_div)?;
        self.spi.unlock(chip)?;
        // Technically we would save a write by not calling disable here, however in the case where
        // we are using the arc messages the ARC code will call disable requiring another init. It
        // feels a bit safer therefore to always init before each read/write step.
        self.spi.disable(chip)?;

        // In order to ensure that the spi is locked even if we fail our write. We use the lambda
        // to store the result of the workload, regardless of the final result.
        let write_result: Result<(), Box<dyn std::error::Error>> = (|| {
            if let (true, Some(spi_addr)) = (self.use_arc, Self::get_spi_dump_address(chip)?) {
                self.spi_arc_write(chip, spi_addr, addr, write)?;
            } else {
                self.spi.init(chip, clock_div)?;
                self.spi.write(chip, addr, write)?;
                self.spi.disable(chip)?;
            }

            Ok(())
        })();

        // If we failed during the write, and then failed during the lock I want to make sure that
        // the original error is returned to the user (it would probably be ideal to chain the
        // error, but I don't expect these calls to suddenly fail).
        let lock_result = (|| {
            self.spi.init(chip, clock_div)?;
            self.spi.lock(chip, 8)?;
            self.spi.disable(chip)?;

            Ok(())
        })();

        // I liked this better than putting `write_result?;`
        #[allow(clippy::question_mark)]
        if let Err(err) = write_result {
            return Err(err);
        }

        // If we didn't hit an error during the write, then return the result of the lock.
        lock_result
    }

    /// Read from the spi, will select the arc read method if available.
    pub fn read(
        &self,
        chip: &impl ChipImpl,
        addr: u32,
        read: &mut [u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let (true, Some(spi_addr)) = (self.use_arc, Self::get_spi_dump_address(chip)?) {
            self.spi_arc_read(chip, spi_addr, addr, read)?;
        } else {
            let clock_div = self.get_clock(chip)?;

            self.spi.init(chip, clock_div)?;
            self.spi.read(chip, addr, read)?;
            self.spi.disable(chip)?;
        }

        Ok(())
    }
}
