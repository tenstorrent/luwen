// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use std::os::fd::AsRawFd;

use crate::{error::PciError, kmdif, BarMapping, PciDevice};

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

impl BarMapping {
    unsafe fn register_address_mut<T>(&self, mut register_addr: u32) -> *mut T {
        let reg_mapping: *mut u8;

        if self.system_reg_mapping.is_some() && register_addr >= self.system_reg_start_offset {
            let mapping = self.system_reg_mapping.as_ref().unwrap_unchecked();

            register_addr -= self.system_reg_offset_adjust;
            reg_mapping = mapping.as_ptr() as *mut u8;
        } else if self.bar0_wc.is_some() && (register_addr as u64) < self.bar0_wc_size {
            let mapping = self.bar0_wc.as_ref().unwrap_unchecked();

            reg_mapping = mapping.as_ptr() as *mut u8;
        } else {
            register_addr -= self.bar0_uc_offset as u32;
            reg_mapping = self.bar0_uc.as_ptr() as *mut u8;
        }

        reg_mapping.offset(register_addr as isize) as *mut T
    }

    unsafe fn register_address<T>(&self, register_addr: u32) -> *const T {
        self.register_address_mut(register_addr) as *const T
    }
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
            let scratch_data = match &self.pci_bar {
                Some(bar) => unsafe {
                    bar.register_address::<u32>(self.read_checking_addr)
                        .read_volatile()
                },
                None => {
                    return Err(PciError::BarUnmapped);
                }
            };

            if scratch_data == ERROR_VALUE {
                return Err(PciError::BrokenConnection);
            }
        }

        Ok(())
    }

    #[inline]
    pub fn read32_no_translation(&self, addr: usize) -> Result<u32, PciError> {
        let data = if addr % core::mem::align_of::<u32>() != 0 {
            unsafe {
                let aligned_read_pointer = addr & !(core::mem::align_of::<u32>() - 1);
                let a = (aligned_read_pointer as *const u32).read_volatile();
                let b = (aligned_read_pointer as *const u32).add(1).read_volatile();

                let byte_offset = addr % core::mem::align_of::<u32>();
                let shift = byte_offset * 8;
                let inverse_shift = (core::mem::size_of::<u32>() * 8) - shift;
                let inverse_mask = (1 << inverse_shift) - 1;
                let _mask = (1 << shift) - 1;

                ((a >> shift) & inverse_mask) | ((b << inverse_shift) & !inverse_mask)
            }
        } else {
            unsafe { (addr as *const u32).read_volatile() }
        };
        self.detect_ffffffff_read(Some(data))?;

        Ok(data)
    }

    #[inline]
    pub fn read32(&self, addr: u32) -> Result<u32, PciError> {
        let read_pointer = match &self.pci_bar {
            Some(bar) => unsafe { bar.register_address::<u32>(addr) as usize },
            None => {
                return Err(PciError::BarUnmapped);
            }
        };
        self.read32_no_translation(read_pointer)
    }

    #[inline]
    pub fn write32_no_translation(&mut self, addr: usize, data: u32) -> Result<(), PciError> {
        if addr % core::mem::align_of::<u32>() != 0 {
            unsafe {
                let aligned_write_pointer = addr & !(core::mem::align_of::<u32>() - 1);
                let a = (aligned_write_pointer as *const u32).read_volatile();
                let b = (aligned_write_pointer as *const u32).add(1).read_volatile();

                let byte_offset = addr % core::mem::align_of::<u32>();
                let shift = byte_offset * 8;
                let inverse_shift = (core::mem::size_of::<u32>() * 8) - shift;
                let inverse_mask = (1 << inverse_shift) - 1;
                let mask = (1 << shift) - 1;

                let a = (a & mask) | ((data & inverse_mask) << shift);
                let b = (b & !mask) | ((data & !inverse_mask) >> inverse_shift);

                (aligned_write_pointer as *mut u32).write_volatile(a);
                (aligned_write_pointer as *mut u32).add(1).write_volatile(b);
            }
        } else {
            unsafe { (addr as *mut u32).write_volatile(data) }
        };
        self.detect_ffffffff_read(None)?;

        Ok(())
    }

    #[inline]
    pub fn write32(&mut self, addr: u32, data: u32) -> Result<(), PciError> {
        let write_pointer = match &self.pci_bar {
            Some(bar) => unsafe { bar.register_address::<u32>(addr) as usize },
            None => {
                return Err(PciError::BarUnmapped);
            }
        };
        self.write32_no_translation(write_pointer, data)
    }

    pub fn write_no_dma<T>(&mut self, addr: u32, data: &[T]) -> Result<(), PciError> {
        unsafe {
            let ptr = match &self.pci_bar {
                Some(bar) => bar.register_address_mut::<T>(addr),
                None => {
                    return Err(PciError::BarUnmapped);
                }
            };
            ptr.copy_from_nonoverlapping(data.as_ptr(), data.len());
        }

        Ok(())
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
        })?;

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
            let ptr = match &self.pci_bar {
                Some(bar) => bar.register_address_mut(addr),
                None => {
                    return Err(PciError::BarUnmapped);
                }
            };
            Self::memcpy_to_device(ptr, data);
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

        unsafe {
            let ptr = match &self.pci_bar {
                Some(bar) => bar.register_address_mut(addr),
                None => {
                    return Err(PciError::BarUnmapped);
                }
            };
            Self::memcpy_from_device(data, ptr);
        }

        if data.len() >= std::mem::size_of::<u32>() {
            self.detect_ffffffff_read(Some(unsafe { (data.as_ptr() as *const u32).read() }))?;
        }

        Ok(())
    }

    pub fn write_block_no_dma(&self, addr: u32, data: &[u8]) -> Result<(), PciError> {
        unsafe {
            let ptr = match &self.pci_bar {
                Some(bar) => bar.register_address_mut(addr),
                None => {
                    return Err(PciError::BarUnmapped);
                }
            };
            Self::memcpy_to_device(ptr, data);
        }

        Ok(())
    }

    pub fn read_block_no_dma(&self, addr: u32, data: &mut [u8]) -> Result<(), PciError> {
        unsafe {
            let ptr = match &self.pci_bar {
                Some(bar) => bar.register_address(addr),
                None => {
                    return Err(PciError::BarUnmapped);
                }
            };
            Self::memcpy_from_device(data, ptr);
        }

        if data.len() >= std::mem::size_of::<u32>() {
            self.detect_ffffffff_read(Some(unsafe { (data.as_ptr() as *const u32).read() }))?;
        }

        Ok(())
    }
}

impl PciDevice {
    /// Copy to a memory location mapped to the PciDevice from a buffer passed in by the host.
    /// Both dest and src may be unaligned.
    ///
    /// These the dest address is not bounds checked, passing in an unvalidated pointer may result
    /// in hangs, or system reboots.
    ///
    /// # Safety
    /// This function requires that dest is a value returned by the self.register_address
    /// function.
    pub unsafe fn memcpy_to_device(dest: *mut u8, src: &[u8]) {
        // Memcpy implementations on aarch64 systems seem to generate invalid code which does not
        // properly respect alignment requirements of the aarch64 "memmove" instruction.
        let align = if cfg!(target_arch = "aarch64") {
            4 * core::mem::align_of::<u32>()
        } else {
            core::mem::align_of::<u32>()
        };

        let mut offset = 0;
        while offset < src.len() {
            let bytes_left = src.len() - offset;

            let block_write_length = bytes_left & !(align - 1);

            let dest_misalign = ((dest as usize) + offset) % align;
            let src_misalign = ((src.as_ptr() as usize) + offset) % align;

            // Our device pcie controller requires that we write in a minimum of 4 byte chunks, and
            // that those chunks are aligned to 4 byte boundaries.
            if bytes_left < 4
                || dest_misalign != 0
                || src_misalign != 0
                || block_write_length < align
            {
                let addr = (dest as usize) + offset;
                let byte_offset = addr % core::mem::align_of::<u32>();

                let src_size_bytes = (core::mem::size_of::<u32>() - byte_offset).min(bytes_left);

                let mut src_data = 0u32;
                for i in (offset..(offset + src_size_bytes)).rev() {
                    src_data <<= 8;
                    src_data |= src[i] as u32;
                }

                let to_write = if byte_offset != 0 || src_size_bytes != 4 {
                    // cannot do an unaligned read
                    let dest_data =
                        ((addr & !(core::mem::align_of::<u32>() - 1)) as *mut u32).read();

                    let shift = byte_offset * 8;
                    let src_mask = ((1 << (src_size_bytes * 8)) - 1) << shift;

                    /*
                    println!(
                        "{dest_data:x} & {:x} = {:x}",
                        !src_mask,
                        dest_data & !src_mask
                    );

                    println!(
                        "({src_data:x} << {}) & {:x} = {:x}",
                        shift,
                        src_mask,
                        (src_data << shift) & src_mask
                    );
                    */

                    (dest_data & !src_mask) | ((src_data << shift) & src_mask)
                } else {
                    src_data
                };

                // println!("{to_write:x}");

                ((addr & !(core::mem::align_of::<u32>() - 1)) as *mut u32).write_volatile(to_write);

                offset += src_size_bytes;
            } else {
                // Everything is aligned!
                ((dest as usize + offset) as *mut u32).copy_from_nonoverlapping(
                    (src.as_ptr() as usize + offset) as *const u32,
                    block_write_length / core::mem::size_of::<u32>(),
                );
                offset += block_write_length;
            }
        }
    }

    /// Copy from a memory location mapped to the PciDevice to a buffer passed in by the host.
    /// Both dest and src may be unaligned.
    ///
    /// These the src address is not bounds checked, passing in an unvalidated pointer may result
    /// in hangs, or system reboots.
    ///
    /// # Safety
    /// This function requires that dest is a value returned by the self.register_address
    /// function.
    pub unsafe fn memcpy_from_device(dest: &mut [u8], src: *const u8) {
        let align = core::mem::align_of::<u32>();

        let mut offset = 0;
        while offset < dest.len() {
            let bytes_left = dest.len() - offset;

            let block_write_length = bytes_left & !(core::mem::align_of::<u32>() - 1);

            let dest_misalign = ((dest.as_ptr() as usize) + offset) % align;
            let src_misalign = ((src as usize) + offset) % align;

            // Our device pcie controller requires that we read in a minimum of 4 byte chunks, and
            // that those chunks are aligned to 4 byte boundaries.
            if bytes_left < 4
                || dest_misalign != 0
                || src_misalign != 0
                || block_write_length < align
            {
                let addr = (src as usize) + offset;
                let byte_offset = addr % core::mem::align_of::<u32>();
                let shift = byte_offset * 8;

                let src_data = ((addr & !(core::mem::align_of::<u32>() - 1)) as *mut u32).read();
                let read = src_data >> shift;

                let read_count = (core::mem::size_of::<u32>() - byte_offset).min(bytes_left);

                let read = read.to_le_bytes();
                dest[offset..(read_count + offset)].copy_from_slice(&read[..read_count]);

                offset += read_count
            } else {
                // Everything is aligned!
                ((dest.as_ptr() as usize + offset) as *mut u32).copy_from_nonoverlapping(
                    (src as usize + offset) as *const u32,
                    block_write_length / core::mem::size_of::<u32>(),
                );
                offset += block_write_length;
            }
        }
    }
}
