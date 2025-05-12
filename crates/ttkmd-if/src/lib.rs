// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    os::{
        fd::{AsRawFd, RawFd},
        unix::prelude::FileTypeExt,
    },
    sync::Arc,
};

mod error;
pub mod ioctl;
mod kmdif;
mod pci;
pub mod tlb;

pub use error::{PciError, PciOpenError};
use ioctl::{
    query_mappings, AllocateDmaBuffer, GetDeviceInfo, GetDeviceInfoOut, Mapping, QueryMappings,
};
use luwen_core::Arch;
pub use tlb::{DeviceTlbInfo, Tlb};

impl From<&GetDeviceInfoOut> for Arch {
    fn from(value: &GetDeviceInfoOut) -> Self {
        match value.device_id {
            0xfaca => Arch::Grayskull,
            0x401e => Arch::Wormhole,
            0xb140 => Arch::Blackhole,
            id => Arch::Unknown(id),
        }
    }
}

pub struct DmaBuffer {
    pub buffer: memmap2::MmapMut,
    pub physical_address: u64,
    pub size: u64,
}

#[derive(Clone)]
pub struct DmaConfig {
    /// Address in CSM where the DMA request structure resides
    pub csm_pcie_ctrl_dma_request_offset: u32,

    /// To trigger ARC interrupt
    pub arc_misc_cntl_addr: u32,

    /// DMA host phys addr high
    pub dma_host_phys_addr_high: u32,

    pub support_64_bit_dma: bool,

    pub use_msi_for_dma: bool,

    pub read_threshold: u32,
    pub write_threshold: u32,
}

pub struct PhysicalDevice {
    pub vendor_id: u16,
    pub device_id: u16,
    pub subsystem_vendor_id: u16,
    pub subsystem_id: u16,

    pub pci_bus: u16,
    pub slot: u16,
    pub pci_function: u16,
    pub pci_domain: u16,

    pub bar_addr: u64,
    pub bar_size_bytes: u64,
}

#[allow(dead_code)]
pub struct PciDevice {
    pub id: usize,

    pub physical: PhysicalDevice,
    pub arch: Arch,

    pub read_checking_enabled: bool,
    pub read_checking_addr: u32,

    next_dma_buf: usize,

    device_fd: std::fs::File,
    bar0_uc: memmap2::MmapMut,
    #[allow(dead_code)]
    bar0_uc_size: u64,
    bar0_uc_offset: u64,

    bar0_wc: Option<memmap2::MmapMut>,
    bar0_wc_size: u64,

    bar1_uc: Option<memmap2::MmapMut>,
    bar1_uc_size: u64,

    config_space: std::fs::File,

    max_dma_buf_size_log2: u16,

    system_reg_mapping: Option<memmap2::MmapMut>,
    #[allow(dead_code)]
    system_reg_mapping_size: usize,
    system_reg_start_offset: u32, // Registers >= this are system regs, use the mapping.
    system_reg_offset_adjust: u32, // This is the offset of the first reg in the system reg mapping.

    #[allow(dead_code)]
    dma_buffer_mappings: Vec<Arc<DmaBuffer>>,
    completion_flag_buffer: Option<DmaBuffer>,
    transfer_buffer: Option<DmaBuffer>,

    pub dma_config: Option<DmaConfig>,
}

fn allocate_dma_buffer(
    device_id: usize,
    device_fd: RawFd,
    max_dma_buf_size_log2: u32,
    buffer_index: usize,
    size: u32,
) -> Result<DmaBuffer, PciError> {
    let mut allocate_dma_buf = AllocateDmaBuffer::default();
    allocate_dma_buf.input.requested_size =
        (size.min(1 << max_dma_buf_size_log2)).max(kmdif::getpagesize().unwrap() as u32);
    allocate_dma_buf.input.buf_index = buffer_index as u8;

    if let Err(err) = unsafe { ioctl::allocate_dma_buffer(device_fd, &mut allocate_dma_buf) } {
        return Err(PciError::DmaAllocationFailed {
            id: device_id,
            size: allocate_dma_buf.input.requested_size,
            err,
        });
    }

    let map = unsafe {
        memmap2::MmapOptions::default()
            .len(allocate_dma_buf.output.size as usize)
            .offset(allocate_dma_buf.output.mapping_offset)
            .map_mut(device_fd)
    };
    let map = match map {
        Err(err) => {
            return Err(PciError::DmaBufferMappingFailed {
                id: device_id,
                source: err,
            })
        }
        Ok(map) => map,
    };

    let output = DmaBuffer {
        buffer: map,
        physical_address: allocate_dma_buf.output.physical_address,
        size: allocate_dma_buf.output.size as u64,
    };

    Ok(output)
}

impl PciDevice {
    pub fn open(device_id: usize) -> Result<PciDevice, PciOpenError> {
        let fd = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(format!("/dev/tenstorrent/{device_id}"));
        let fd = match fd {
            Ok(fd) => fd,
            Err(err) => {
                return Err(PciOpenError::DeviceOpenFailed {
                    id: device_id,
                    source: err,
                })
            }
        };

        let mut device_info = GetDeviceInfo::default();
        device_info.input.output_size_bytes = std::mem::size_of::<ioctl::GetDeviceInfoOut>() as u32;

        if let Err(errorno) = unsafe { ioctl::get_device_info(fd.as_raw_fd(), &mut device_info) } {
            return Err(PciOpenError::IoctlError {
                name: "get_device_info".to_string(),
                id: device_id,
                source: errorno,
            });
        }

        let max_dma_buf_size_log2 = device_info.output.max_dma_buf_size_log2;

        let mut mappings = QueryMappings::<8>::default();

        if let Err(erno) = unsafe { query_mappings(fd.as_raw_fd(), &mut mappings) } {
            return Err(PciOpenError::IoctlError {
                name: "query_mappings".to_string(),
                id: device_id,
                source: erno,
            });
        }

        let arch = Arch::from(&device_info.output);

        let mut bar0_uc_mapping = Mapping::default();
        let mut bar0_wc_mapping = Mapping::default();
        let mut bar1_uc_mapping = Mapping::default();
        let mut _bar1_wc_mapping = Mapping::default();
        let mut bar2_uc_mapping = Mapping::default();
        let mut _bar2_wc_mapping = Mapping::default();

        for i in 0..mappings.input.output_mapping_count as usize {
            match kmdif::MappingId::from_u32(mappings.output.mappings[i].mapping_id) {
                kmdif::MappingId::Resource0Uc => {
                    bar0_uc_mapping = mappings.output.mappings[i];
                }
                kmdif::MappingId::Resource0Wc => {
                    bar0_wc_mapping = mappings.output.mappings[i];
                }
                kmdif::MappingId::Resource1Uc => {
                    bar1_uc_mapping = mappings.output.mappings[i];
                }
                kmdif::MappingId::Resource1Wc => {
                    _bar1_wc_mapping = mappings.output.mappings[i];
                }
                kmdif::MappingId::Resource2Uc => {
                    bar2_uc_mapping = mappings.output.mappings[i];
                }
                kmdif::MappingId::Resource2Wc => {
                    _bar2_wc_mapping = mappings.output.mappings[i];
                }
                kmdif::MappingId::Unused => {
                    // println!("WARNING: recieved unused mapping id");
                }
                kmdif::MappingId::Unknown(v) => {
                    println!("WARNING: recieved unknown mapping id {v}");
                }
            }
        }

        if bar0_uc_mapping.mapping_id != kmdif::MappingId::Resource0Uc.as_u32() {
            return Err(PciOpenError::BarMappingError {
                name: "bar0_uc_mapping".to_string(),
                id: device_id,
            });
        }

        let wc_mapping_size = if arch.is_blackhole() {
            kmdif::BH_BAR0_WC_MAPPING_SIZE
        } else {
            kmdif::GS_BAR0_WC_MAPPING_SIZE
        };

        // if disable_wc
        // bar0_wc_mapping = 0;
        // bar0_wc_size = 0;
        // else
        let mut bar0_wc_size = 0;
        let mut bar0_wc = None;
        if bar0_wc_mapping.mapping_id == kmdif::MappingId::Resource0Wc.as_u32() {
            bar0_wc_size = bar0_wc_mapping.mapping_size.min(wc_mapping_size);
            let bar0_wc_map = unsafe {
                memmap2::MmapOptions::default()
                    .len(bar0_wc_size as usize)
                    .offset(bar0_wc_mapping.mapping_base)
                    .map_mut(fd.as_raw_fd())
            };
            match bar0_wc_map {
                Ok(map) => {
                    bar0_wc = Some(map);
                }
                Err(err) => {
                    println!("WARNING: Failed to map bar0_wc for {device_id} with error {err}");
                    bar0_wc_size = 0;
                    bar0_wc = None;
                }
            }
        }

        let bar0_uc_size;
        let bar0_uc_offset;
        if bar0_wc.is_some() {
            bar0_uc_size = bar0_uc_mapping.mapping_size.saturating_sub(wc_mapping_size);
            bar0_uc_offset = wc_mapping_size;
        } else {
            // No WC mapping, map the entire BAR UC.
            bar0_uc_size = bar0_uc_mapping.mapping_size;
            bar0_uc_offset = 0;
        }

        let bar0_uc = unsafe {
            memmap2::MmapOptions::default()
                .len(bar0_uc_size as usize)
                .offset(bar0_uc_mapping.mapping_base + bar0_uc_offset)
                .map_mut(fd.as_raw_fd())
        };
        let bar0_uc = match bar0_uc {
            Ok(map) => map,
            Err(err) => {
                panic!("Failed to map bar0_uc for {device_id} with error {err}");
            }
        };

        // let bar0_wc = if let Some(bar0_wc) = bar0_wc {
        //     bar0_wc
        // } else {
        //     bar0_uc
        // };

        let mut system_reg_mapping_size = 0;
        let mut system_reg_start_offset = 0;
        let mut system_reg_offset_adjust = 0;
        let mut system_reg_mapping = None;
        if arch.is_wormhole() {
            if bar2_uc_mapping.mapping_id != kmdif::MappingId::Resource2Uc.as_u32() {
                panic!("Device {device_id} has no BAR4 mapping");
            }

            system_reg_mapping_size = bar2_uc_mapping.mapping_size as usize;
            let system_reg = unsafe {
                memmap2::MmapOptions::default()
                    .len(system_reg_mapping_size)
                    .offset(bar2_uc_mapping.mapping_base)
                    .map_mut(fd.as_raw_fd())
            };
            system_reg_mapping = match system_reg {
                Ok(map) => Some(map),
                Err(err) => {
                    panic!("BAR4 mapping failed for device {device_id} with error {err}");
                }
            };

            system_reg_start_offset = (512 - 16) * 1024 * 1024;
            system_reg_offset_adjust = (512 - 32) * 1024 * 1024;
        }

        let mut bar1_uc = None;
        let mut bar1_uc_size = 0;
        if arch.is_blackhole() {
            if bar1_uc_mapping.mapping_id != kmdif::MappingId::Resource1Uc.as_u32() {
                panic!("Device {device_id} has not Bar1 UC mapping");
            }

            bar1_uc_size = bar1_uc_mapping.mapping_size;
            bar1_uc = Some(unsafe {
                memmap2::MmapOptions::default()
                    .len(bar1_uc_mapping.mapping_size as usize)
                    .offset(bar1_uc_mapping.mapping_base)
                    .map_mut(fd.as_raw_fd())
                    .expect("Bar1 mapping failed for device {device_id}")
            });
        }

        let pci_bus = device_info.output.bus_dev_fn >> 8;
        let slot = ((device_info.output.bus_dev_fn) >> 3) & 0x1f; // The definition of PCI_SLOT from include/uapi/linux/pci.h
        let pci_function = (device_info.output.bus_dev_fn) & 0x7; // The definition of PCI_FUNC from include/uapi/linux/pci.h
        let pci_domain = device_info.output.pci_domain;

        let config_space = std::fs::OpenOptions::new()
            .read(true)
            .write(false)
            .open(format!(
                "/sys/bus/pci/devices/{:04x}:{:02x}:{:02x}.{:01x}/config",
                pci_domain, pci_bus, slot, pci_function
            ));
        let config_space = match config_space {
            Ok(file) => file,
            Err(err) => {
                panic!("Failed to open config space for device {device_id} with error {err}");
            }
        };

        let mut device = PciDevice {
            id: device_id,
            arch,

            physical: PhysicalDevice {
                vendor_id: device_info.output.vendor_id,
                device_id: device_info.output.device_id,
                subsystem_vendor_id: device_info.output.subsystem_vendor_id,
                subsystem_id: device_info.output.subsystem_id,
                pci_bus,
                slot,
                pci_function,
                pci_domain,
                bar_addr: pci::read_bar0_base(&config_space),
                bar_size_bytes: bar0_uc_mapping.mapping_size,
            },

            read_checking_enabled: true,
            read_checking_addr: if arch.is_blackhole() {
                kmdif::BH_NOC_NODE_ID_OFFSET
            } else {
                kmdif::GS_WH_ARC_SCRATCH6_ADDR
            },

            next_dma_buf: 0,

            device_fd: fd,

            bar0_uc,
            bar0_uc_size,
            bar0_uc_offset,

            bar0_wc,
            bar0_wc_size,

            bar1_uc,
            bar1_uc_size,

            config_space,

            max_dma_buf_size_log2,

            system_reg_mapping,
            system_reg_mapping_size,
            system_reg_start_offset,
            system_reg_offset_adjust,

            dma_buffer_mappings: vec![],
            dma_config: None,
            completion_flag_buffer: None,
            transfer_buffer: None,
        };

        // To avoid needing a warmup when performing the first dma access, try to allocate the
        // buffers now.
        device.allocate_transfer_buffers();

        Ok(device)
    }

    pub fn allocate_transfer_buffers(&mut self) -> bool {
        // Try to allocate the transfer buffer first, if this fails then there is no point in
        // allocating the completion flag.
        if self.transfer_buffer.is_none() {
            self.transfer_buffer = self
                .allocate_dma_buffer_range(
                    kmdif::getpagesize().unwrap() as u32,
                    kmdif::MAX_DMA_BYTES,
                )
                .ok();
        }

        // If we didn't get the transfer buffer then there is no point in allocating the completion
        // flag
        if self.transfer_buffer.is_some() && self.completion_flag_buffer.is_none() {
            self.completion_flag_buffer = self
                .allocate_dma_buffer(std::mem::size_of::<u64>() as u32)
                .ok();
        }

        self.transfer_buffer.is_some() && self.completion_flag_buffer.is_some()
    }

    pub fn allocate_dma_buffer_range(
        &mut self,
        min_size: u32,
        max_size: u32,
    ) -> Result<DmaBuffer, PciError> {
        let page_size = kmdif::getpagesize().unwrap() as u32;

        let mut page_aligned_size = (max_size + page_size - 1) & !(page_size - 1);
        let min_aligned_page_size = (min_size + page_size - 1) & !(page_size - 1);

        loop {
            match allocate_dma_buffer(
                self.id,
                self.device_fd.as_raw_fd(),
                self.max_dma_buf_size_log2 as u32,
                self.next_dma_buf,
                page_aligned_size,
            ) {
                Ok(buf) => {
                    self.next_dma_buf += 1;
                    return Ok(buf);
                }
                Err(err) => {
                    if page_aligned_size <= min_aligned_page_size {
                        return Err(err);
                    }

                    page_aligned_size = (page_aligned_size / 2).max(min_aligned_page_size);
                }
            }
        }
    }

    pub fn allocate_dma_buffer(&mut self, size: u32) -> Result<DmaBuffer, PciError> {
        self.allocate_dma_buffer_range(size, size)
    }

    pub fn resource_lock(&self, index: u8) -> Result<bool, PciError> {
        let mut data = ioctl::LockCtl {
            input: ioctl::LockCtlIn {
                flags: ioctl::LOCK_CTL_ACQUIRE,
                index,
                ..Default::default()
            },
            ..Default::default()
        };

        let result = unsafe { ioctl::lock_ctl(self.device_fd.as_raw_fd(), (&mut data) as *mut _) };

        match result {
            Ok(value) => {
                if value == 1 {
                    Ok(true)
                } else if value == 0 {
                    Ok(false)
                } else {
                    Err(PciError::IoctlError(nix::errno::Errno::EINVAL))
                }
            }
            Err(errno) => Err(PciError::IoctlError(errno)),
        }
    }

    pub fn resource_unlock(&self, index: u8) -> Result<bool, PciError> {
        let mut data = ioctl::LockCtl {
            input: ioctl::LockCtlIn {
                flags: ioctl::LOCK_CTL_RELEASE,
                index,
                ..Default::default()
            },
            ..Default::default()
        };

        let result = unsafe { ioctl::lock_ctl(self.device_fd.as_raw_fd(), (&mut data) as *mut _) };

        match result {
            Ok(value) => {
                if value == 1 {
                    Ok(true)
                } else if value == 0 {
                    Ok(false)
                } else {
                    Err(PciError::IoctlError(nix::errno::Errno::EINVAL))
                }
            }
            Err(errno) => Err(PciError::IoctlError(errno)),
        }
    }

    pub fn resource_test(&self, index: u8) -> Result<bool, PciError> {
        let mut data = ioctl::LockCtl {
            input: ioctl::LockCtlIn {
                flags: ioctl::LOCK_CTL_TEST,
                index,
                ..Default::default()
            },
            ..Default::default()
        };

        let result = unsafe { ioctl::lock_ctl(self.device_fd.as_raw_fd(), (&mut data) as *mut _) };

        match result {
            Ok(value) => {
                if value != 0 {
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            Err(errno) => Err(PciError::IoctlError(errno)),
        }
    }

    pub fn allocate_tlb(&self, size: u64) -> Result<TlbAllocation, PciError> {
        let mut data = ioctl::AllocateTlb {
            input: ioctl::AllocateTlbIn {
                size,
                ..Default::default()
            },
            ..Default::default()
        };
        let result =
            unsafe { ioctl::allocate_tlb(self.device_fd.as_raw_fd(), (&mut data) as *mut _) };

        let uc_mapping = unsafe {
            memmap2::MmapOptions::default()
                .len(size as usize)
                .offset(data.output.mmap_offset_uc)
                .map_mut(self.device_fd.as_raw_fd())
        }
        .map_err(|_err| PciError::TlbAllocationError("Failed to map uc buffer".to_string()))?;

        let wc_mapping = unsafe {
            memmap2::MmapOptions::default()
                .len(size as usize)
                .offset(data.output.mmap_offset_wc)
                .map_mut(self.device_fd.as_raw_fd())
        }
        .map_err(|_err| PciError::TlbAllocationError("Failed to map wc buffer".to_string()))?;

        match result {
            Ok(rc) => match rc {
                0 => Ok(TlbAllocation {
                    id: data.output.id,
                    uc_mapping,
                    wc_mapping,
                    size,
                }),
                errno => Err(PciError::IoctlError(nix::errno::Errno::from_i32(errno))),
            },
            Err(errno) => Err(PciError::IoctlError(errno)),
        }
    }

    pub fn free_tlb(&self, alloc: &TlbAllocation) -> Result<bool, PciError> {
        let result = unsafe {
            ioctl::free_tlb(
                self.device_fd.as_raw_fd(),
                (&mut ioctl::FreeTlb {
                    input: ioctl::FreeTlbIn { id: alloc.id },
                    output: ioctl::FreeTlbOut {},
                }) as *mut _,
            )
        };

        match result {
            Ok(rc) => match rc {
                0 => Ok(true),
                _ => Ok(false),
            },
            Err(errno) => match errno {
                nix::errno::Errno::EINVAL => Ok(false),
                errno => Err(PciError::IoctlError(errno)),
            },
        }
    }

    pub fn configure_tlb(
        &self,
        alloc: &TlbAllocation,
        config: ioctl::NocTlbConfig,
    ) -> Result<bool, PciError> {
        let result = unsafe {
            ioctl::configure_tlb(
                self.device_fd.as_raw_fd(),
                (&mut ioctl::ConfigureTlb {
                    input: ioctl::ConfigureTlbIn {
                        id: alloc.id,
                        config,
                    },
                    output: ioctl::ConfigureTlbOut {
                        ..Default::default()
                    },
                }) as *mut _,
            )
        };

        match result {
            Ok(rc) => match rc {
                0 => Ok(true),
                _ => Ok(false),
            },
            Err(errno) => match errno {
                nix::errno::Errno::EINVAL => Ok(false),
                errno => Err(PciError::IoctlError(errno)),
            },
        }
    }

    pub fn scan() -> Vec<usize> {
        let output = std::fs::read_dir("/dev/tenstorrent");
        let output = match output {
            Ok(output) => output,
            Err(err) => {
                tracing::debug!("When reading /dev/tenstorrent for a scan hit error: {err}");
                return Vec::new();
            }
        };

        let mut output = output
            .filter_map(|entry| {
                let entry = entry.ok()?;

                if !entry.file_type().ok()?.is_char_device() {
                    return None;
                }

                let path = entry.path();
                let file_name = path.file_name()?.to_str()?;
                file_name.parse::<usize>().ok()
            })
            .collect::<Vec<_>>();

        output.sort();

        output
    }
}
