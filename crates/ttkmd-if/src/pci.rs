// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use std::os::fd::AsRawFd;

use crate::{error::PciError, kmdif, PciDevice};

const ERROR_VALUE: u32 = 0xffffffff;

pub(crate) fn read_bar0_base(config_space: &std::fs::File) -> u64 {
    const BAR_ADDRESS_MASK: u64 = !0xFu64;

    let bar0_config_offset = 0x10;

    let mut bar01 = [0u8; std::mem::size_of::<u64>()];
    let size = nix::sys::uio::pread(config_space.as_raw_fd(), &mut bar01, bar0_config_offset);
    match size {
        Ok(size) => {
            if size != std::mem::size_of::<u64>() {
                panic!("Failed to read BAR0 config space: {}", size);
            }
        }
        Err(err) => {
            panic!("Failed to read BAR0 config space: {}", err);
        }
    }

    u64::from_ne_bytes(bar01) & BAR_ADDRESS_MASK
}

impl PciDevice {
    pub fn read_cfg(&self, byte_offset: u32, data: &mut [u8]) -> Result<(), PciError> {
        let size = nix::sys::uio::pread(self.config_space.as_raw_fd(), data, byte_offset as i64);
        match size {
            Ok(size) => {
                if size != data.len() {
                    return Err(PciError::CfgReadFailed {
                        id: self.id,
                        offset: byte_offset as usize,
                        size: data.len(),
                        source: crate::error::CfgFailType::SizeMismatch(size),
                    });
                }
            }
            Err(err) => {
                return Err(PciError::CfgReadFailed {
                    id: self.id,
                    offset: byte_offset as usize,
                    size: data.len(),
                    source: crate::error::CfgFailType::Nix(err),
                });
            }
        }

        Ok(())
    }

    pub fn write_cfg(&self, byte_offset: u32, data: &[u8]) -> Result<(), PciError> {
        let size = nix::sys::uio::pwrite(self.config_space.as_raw_fd(), data, byte_offset as i64);
        match size {
            Ok(size) => {
                if size != data.len() {
                    return Err(PciError::CfgWriteFailed {
                        id: self.id,
                        offset: byte_offset as usize,
                        size: data.len(),
                        source: crate::error::CfgFailType::SizeMismatch(size),
                    });
                }
            }
            Err(err) => {
                return Err(PciError::CfgWriteFailed {
                    id: self.id,
                    offset: byte_offset as usize,
                    size: data.len(),
                    source: crate::error::CfgFailType::Nix(err),
                });
            }
        }

        Ok(())
    }

    #[inline]
    pub fn detect_ffffffff_read(&self, data_read: Option<u32>) -> Result<(), PciError> {
        let data_read = data_read.unwrap_or(ERROR_VALUE);

        if self.read_checking_enabled && data_read == ERROR_VALUE {
            const ARC_SCRATCH6_ADDR: u32 = 0x1ff30078;
            let scratch_data = unsafe {
                self.register_address::<u32>(ARC_SCRATCH6_ADDR)
                    .read_volatile()
            };

            if scratch_data == ERROR_VALUE {
                return Err(PciError::BrokenConnection);
            }
        }

        Ok(())
    }

    unsafe fn register_address<T>(&self, mut register_addr: u32) -> *const T {
        let reg_mapping: *const u8;

        if self.system_reg_mapping.is_some() && register_addr >= self.system_reg_start_offset {
            let mapping = self.system_reg_mapping.as_ref().unwrap_unchecked();

            register_addr -= self.system_reg_offset_adjust;
            reg_mapping = mapping.as_ptr();
        } else if self.bar0_wc.is_some() && (register_addr as usize) < self.bar0_wc_size {
            let mapping = self.bar0_wc.as_ref().unwrap_unchecked();

            reg_mapping = mapping.as_ptr();
        } else {
            register_addr -= self.bar0_uc_offset as u32;
            reg_mapping = self.bar0_uc.as_ptr();
        }

        reg_mapping.offset(register_addr as isize) as *const T
    }

    unsafe fn register_address_mut<T>(&self, mut register_addr: u32) -> *mut T {
        let reg_mapping: *mut u8;

        if self.system_reg_mapping.is_some() && register_addr >= self.system_reg_start_offset {
            let mapping = self.system_reg_mapping.as_ref().unwrap_unchecked();

            register_addr -= self.system_reg_offset_adjust;
            reg_mapping = mapping.as_ptr() as *mut u8;
        } else if self.bar0_wc.is_some() && (register_addr as usize) < self.bar0_wc_size {
            let mapping = self.bar0_wc.as_ref().unwrap_unchecked();

            reg_mapping = mapping.as_ptr() as *mut u8;
        } else {
            register_addr -= self.bar0_uc_offset as u32;
            reg_mapping = self.bar0_uc.as_ptr() as *mut u8;
        }

        reg_mapping.offset(register_addr as isize) as *mut T
    }

    #[inline]
    pub fn read32(&self, addr: u32) -> Result<u32, PciError> {
        let data = unsafe { self.register_address::<u32>(addr).read_volatile() };
        self.detect_ffffffff_read(Some(data))?;

        Ok(data)
    }

    #[inline]
    pub fn write32(&mut self, addr: u32, data: u32) -> Result<(), PciError> {
        unsafe { self.register_address_mut::<u32>(addr).write_volatile(data) };
        self.detect_ffffffff_read(None)?;

        Ok(())
    }

    pub fn write_no_dma<T>(&mut self, addr: u32, data: &[T]) {
        unsafe {
            let ptr = self.register_address_mut::<T>(addr);
            ptr.copy_from_nonoverlapping(data.as_ptr(), data.len());
        }
    }
}

impl PciDevice {
    // HACK(drosen): Yes user data should be a mut slice,
    // but I don't really want to refactor the code righ now to make that possible
    pub fn pcie_dma_transfer_turbo(
        &mut self,
        chip_addr: u32,
        host_buffer_addr: u64,
        size: u32,
        write: bool,
    ) -> Result<(), PciError> {
        if self.dma_config.is_none() || !self.allocate_transfer_buffers() {
            return Err(PciError::DmaNotConfigured { id: self.id });
        }

        let dma_config = unsafe { self.dma_config.as_ref().unwrap_unchecked().clone() };

        let host_phys_addr_hi = (host_buffer_addr >> 32) as u32;

        if host_phys_addr_hi != 0 && !dma_config.support_64_bit_dma {
            return Err(PciError::No64bitDma { id: self.id });
        }

        if size > (1 << 28) - 1 {
            return Err(PciError::DmaTooLarge {
                id: self.id,
                size: size as usize,
            });
        }

        // SAFETY: Already checked that the completion_flag_buffer is Some in
        // self.allocate_transfer_buffers
        let completion_flag_buffer =
            unsafe { self.completion_flag_buffer.as_mut().unwrap_unchecked() };
        let req = kmdif::ArcPcieCtrlDmaRequest {
            chip_addr,
            host_phys_addr_lo: (host_buffer_addr & 0xffffffff) as u32,
            completion_flag_phys_addr: completion_flag_buffer.physical_address as u32,
            dma_pack: kmdif::DmaPack::new()
                .with_size_bytes(size)
                .with_write(write)
                .with_pcie_msi_on_done(dma_config.use_msi_for_dma)
                .with_pcie_write_on_done(!dma_config.use_msi_for_dma)
                .with_trigger(true),
            repeat: 1 | (((host_phys_addr_hi != 0) as u32) << 31), // 64-bit PCIe DMA transfer request
        };

        let complete_flag = completion_flag_buffer.buffer.as_ptr() as *mut u32;
        unsafe { complete_flag.write_volatile(0) };

        // Configure the DMA engine
        if dma_config.support_64_bit_dma {
            self.write32(dma_config.dma_host_phys_addr_high, host_phys_addr_hi)?;
        }

        let config_addr = dma_config.csm_pcie_ctrl_dma_request_offset;

        assert!(config_addr % 4 == 0);
        self.write_no_dma(config_addr, unsafe {
            std::slice::from_raw_parts(
                &req as *const _ as *const u32,
                std::mem::size_of::<kmdif::ArcPcieCtrlDmaRequest>() / 4,
            )
        });

        // Trigger ARC interrupt 0 on core 0
        let mut arc_misc_cntl_value = 0;

        // NOTE: Ideally, we should read the state of this register before writing to it, but that
        //       casues a lot of delay (reads have huge latencies)
        arc_misc_cntl_value |= 1 << 16; // Cause IRQ0 on core 0
        self.write32(dma_config.arc_misc_cntl_addr, arc_misc_cntl_value)?;

        if !dma_config.use_msi_for_dma {
            loop {
                // The complete flag is set ty by ARC (see src/hardware/soc/tb/arc_fw/lib/pcie_dma.c)
                unsafe {
                    if complete_flag.read_volatile() == 0xfaca {
                        break;
                    }
                }
            }
        } else {
            unimplemented!("Do not currently support MSI based dma");
        }

        Ok(())
    }

    pub fn write_block(&mut self, addr: u32, data: &[u8]) -> Result<(), PciError> {
        if let Some(dma_config) = self.dma_config.clone() {
            #[allow(clippy::collapsible_if)] // I want to make it clear that these are seperate
            // types of checks
            if data.len() > dma_config.write_threshold as usize && dma_config.write_threshold > 0 {
                if self.allocate_transfer_buffers() {
                    let mut num_bytes = data.len();
                    let mut offset = 0;
                    while num_bytes > 0 {
                        // SAFETY: Already checked that the transfer_buffer is Some in
                        // self.allocate_transfer_buffers
                        let buffer = unsafe { self.transfer_buffer.as_mut().unwrap_unchecked() };

                        let chunk_size = num_bytes.min(buffer.size as usize);
                        buffer.buffer[..chunk_size]
                            .copy_from_slice(&data[offset..(offset + chunk_size)]);

                        // SAFETY: Already checked that the transfer_buffer is Some in
                        // self.allocate_transfer_buffers
                        let buffer_addr =
                            unsafe { self.transfer_buffer.as_mut().unwrap_unchecked() }
                                .physical_address;
                        self.pcie_dma_transfer_turbo(
                            addr + offset as u32,
                            buffer_addr,
                            chunk_size as u32,
                            true,
                        )?;
                        num_bytes = num_bytes.saturating_sub(chunk_size);
                        offset += chunk_size;
                    }

                    return Ok(());
                }
            }
        }

        unsafe {
            Self::memcpy_to_device(self.register_address_mut(addr), data);
        }

        Ok(())
    }

    pub fn read_block(&mut self, addr: u32, data: &mut [u8]) -> Result<(), PciError> {
        if let Some(dma_config) = self.dma_config.clone() {
            #[allow(clippy::collapsible_if)] // I want to make it clear that these are seperate
            // types of checks
            if data.len() > dma_config.read_threshold as usize && dma_config.read_threshold > 0 {
                if self.allocate_transfer_buffers() {
                    let mut num_bytes = data.len();
                    let mut offset = 0;
                    while num_bytes > 0 {
                        // SAFETY: Already checked that the transfer_buffer is Some in
                        // self.allocate_transfer_buffers
                        let buffer = unsafe { self.transfer_buffer.as_ref().unwrap_unchecked() };

                        let chunk_size = num_bytes.min(buffer.size as usize);

                        self.pcie_dma_transfer_turbo(
                            addr + offset as u32,
                            buffer.physical_address,
                            chunk_size as u32,
                            false,
                        )?;

                        // SAFETY: Already checked that the transfer_buffer is Some in
                        // self.allocate_transfer_buffers
                        let buffer = self.transfer_buffer.as_ref().unwrap();
                        data[offset..(offset + chunk_size)]
                            .copy_from_slice(&buffer.buffer[..chunk_size]);
                        num_bytes = num_bytes.saturating_sub(chunk_size);
                        offset += chunk_size;
                    }

                    return Ok(());
                }
            }
        }

        Self::memcpy_from_device(data, unsafe { self.register_address(addr) });

        if data.len() >= std::mem::size_of::<u32>() {
            self.detect_ffffffff_read(Some(unsafe { (data.as_ptr() as *const u32).read() }))?;
        }

        Ok(())
    }
}

impl PciDevice {
    /// # Safety
    /// This function requires that dest is a value gotten from the self.register_address
    /// function.
    pub unsafe fn memcpy_to_device(dest: *mut u8, src: &[u8]) {
        // Start by aligning the destination (device) pointer. If needed, do RMW to fix up the
        // first partial word.
        let dest_misalignment = dest as usize % std::mem::size_of::<u32>();

        let (dest, src) = if dest_misalignment != 0 {
            // Read-modify-write for the first dest element.
            let dest = unsafe { dest.offset(-(dest_misalignment as isize)) };
            let dest = dest as *mut u32;

            let tmp = unsafe { dest.read() };

            let leading_len = (std::mem::size_of::<u32>() - dest_misalignment).min(src.len());

            unsafe {
                src.as_ptr()
                    .copy_to_nonoverlapping(&tmp as *const u32 as *mut u8, leading_len)
            };

            unsafe { dest.write(tmp) };

            (unsafe { dest.add(1) }, &src[leading_len..])
        } else {
            (dest as *mut u32, src)
        };

        let byte_len = src.len();
        let word_len = byte_len / std::mem::size_of::<u32>();
        let word_src = src.as_ptr() as *const u32;

        unsafe { dest.copy_from_nonoverlapping(word_src, word_len) };

        // Finally copy any sub-word trailer, again RMW on the destination.
        let trailing_len = byte_len % std::mem::size_of::<u32>();
        if trailing_len != 0 {
            let tmp = unsafe { dest.add(word_len).read() };

            let sp = unsafe { word_src.add(word_len) } as *const u8;
            unsafe { sp.copy_to_nonoverlapping(&tmp as *const u32 as *mut u8, trailing_len) };

            unsafe { dest.add(word_len).write(tmp) };
        }
    }

    fn memcpy_from_device(dest: &mut [u8], src: *const u8) {
        type CopyT = u32;

        let mut byte_len = dest.len();
        let src_misalignment = src as usize % std::mem::size_of::<CopyT>();

        let dest = dest.as_mut_ptr();

        let (dest, src) = if src_misalignment != 0 {
            let src = unsafe { src.offset(-(src_misalignment as isize)) };
            let src = src as *const CopyT;

            let tmp = unsafe { src.read() };

            let leading_len = (std::mem::size_of::<CopyT>() - src_misalignment).min(byte_len);
            unsafe {
                dest.copy_from_nonoverlapping(
                    (&tmp as *const u32 as *const u8).add(src_misalignment),
                    leading_len,
                )
            };
            byte_len -= leading_len;

            (unsafe { dest.add(leading_len) }, src)
        } else {
            (dest, src as *const CopyT)
        };

        let word_len = byte_len / std::mem::size_of::<CopyT>();
        let dest = dest as *mut CopyT;
        unsafe { dest.copy_from_nonoverlapping(src, word_len) };

        let trailing_len = byte_len % std::mem::size_of::<CopyT>();
        if trailing_len != 0 {
            let tmp = unsafe { src.add(word_len).read() };
            unsafe {
                (dest.add(word_len) as *mut u8)
                    .copy_from_nonoverlapping(&tmp as *const CopyT as *const u8, trailing_len)
            };
        }
    }
}
