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

impl TryFrom<&GetDeviceInfoOut> for Arch {
    type Error = u16;

    fn try_from(value: &GetDeviceInfoOut) -> Result<Self, Self::Error> {
        match value.device_id {
            0xfaca => Ok(Arch::Grayskull),
            0x401e => Ok(Arch::Wormhole),
            0xb140 => Ok(Arch::Blackhole),
            id => Err(id),
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
}

pub struct BarMapping {
    pub bar_addr: u64,
    pub bar_size_bytes: u64,

    pub bar0_uc: memmap2::MmapMut,
    #[allow(dead_code)]
    pub bar0_uc_size: u64,
    pub bar0_uc_offset: u64,

    pub bar0_wc: Option<memmap2::MmapMut>,
    pub bar0_wc_size: u64,

    pub bar1_uc: Option<memmap2::MmapMut>,
    pub bar1_uc_size: u64,

    pub system_reg_mapping: Option<memmap2::MmapMut>,
    #[allow(dead_code)]
    pub system_reg_mapping_size: usize,
    pub system_reg_start_offset: u32, // Registers >= this are system regs, use the mapping.
    pub system_reg_offset_adjust: u32, // This is the offset of the first reg in the system reg mapping.
}

#[derive(Debug)]
pub struct TlbAllocation {
    pub id: u32,
    pub uc_mapping: memmap2::MmapMut,
    pub size: u64,
}

#[derive(Debug)]
pub enum PossibleTlbAllocation {
    Allocation(TlbAllocation),
    Hardcoded(u32),
    NoAllocation,
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
    pub driver_version: u32,

    config_space: std::fs::File,

    max_dma_buf_size_log2: u16,

    #[allow(dead_code)]
    dma_buffer_mappings: Vec<Arc<DmaBuffer>>,
    completion_flag_buffer: Option<DmaBuffer>,
    transfer_buffer: Option<DmaBuffer>,

    pub dma_config: Option<DmaConfig>,
    pub pci_bar: Option<BarMapping>,
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
    fn map_bar(&mut self) -> Result<(), PciOpenError> {
        let mut mappings = QueryMappings::<8>::default();

        if let Err(erno) = unsafe { query_mappings(self.device_fd.as_raw_fd(), &mut mappings) } {
            return Err(PciOpenError::IoctlError {
                name: "query_mappings".to_string(),
                id: self.id,
                source: erno,
            });
        }

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
                id: self.id,
            });
        }

        let wc_mapping_size = if self.arch.is_blackhole() {
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
                    .map_mut(self.device_fd.as_raw_fd())
            };
            match bar0_wc_map {
                Ok(map) => {
                    bar0_wc = Some(map);
                }
                Err(err) => {
                    println!(
                        "WARNING: Failed to map bar0_wc for {} with error {err}",
                        self.id
                    );
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
                .map_mut(self.device_fd.as_raw_fd())
        };
        let bar0_uc = match bar0_uc {
            Ok(map) => map,
            Err(err) => {
                panic!("Failed to map bar0_uc for {} with error {err}", self.id);
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
        if self.arch.is_wormhole() {
            if bar2_uc_mapping.mapping_id != kmdif::MappingId::Resource2Uc.as_u32() {
                panic!("Device {} has no BAR4 mapping", self.id);
            }

            system_reg_mapping_size = bar2_uc_mapping.mapping_size as usize;
            let system_reg = unsafe {
                memmap2::MmapOptions::default()
                    .len(system_reg_mapping_size)
                    .offset(bar2_uc_mapping.mapping_base)
                    .map_mut(self.device_fd.as_raw_fd())
            };
            system_reg_mapping = match system_reg {
                Ok(map) => Some(map),
                Err(err) => {
                    panic!(
                        "BAR4 mapping failed for device {} with error {err}",
                        self.id
                    );
                }
            };

            system_reg_start_offset = (512 - 16) * 1024 * 1024;
            system_reg_offset_adjust = (512 - 32) * 1024 * 1024;
        }

        let mut bar1_uc = None;
        let mut bar1_uc_size = 0;
        if self.arch.is_blackhole() {
            if bar1_uc_mapping.mapping_id != kmdif::MappingId::Resource1Uc.as_u32() {
                panic!("Device {} has not Bar1 UC mapping", self.id);
            }

            bar1_uc_size = bar1_uc_mapping.mapping_size;
            bar1_uc = Some(unsafe {
                memmap2::MmapOptions::default()
                    .len(bar1_uc_mapping.mapping_size as usize)
                    .offset(bar1_uc_mapping.mapping_base)
                    .map_mut(self.device_fd.as_raw_fd())
                    .expect("Bar1 mapping failed for device {device_id}")
            });
        }

        self.pci_bar = Some(BarMapping {
            bar_addr: pci::read_bar0_base(&self.config_space),
            bar_size_bytes: bar0_uc_mapping.mapping_size,

            bar0_uc,
            bar0_uc_size,
            bar0_uc_offset,
            bar0_wc,
            bar0_wc_size,
            bar1_uc,
            bar1_uc_size,
            system_reg_mapping,
            system_reg_mapping_size,
            system_reg_start_offset,
            system_reg_offset_adjust,
        });

        Ok(())
    }

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
        if let Err(errorno) = unsafe { ioctl::get_device_info(fd.as_raw_fd(), &mut device_info) } {
            return Err(PciOpenError::IoctlError {
                name: "get_device_info".to_string(),
                id: device_id,
                source: errorno,
            });
        }

        let mut driver_info = ioctl::GetDriverInfo::default();
        if let Err(errorno) = unsafe { ioctl::get_driver_info(fd.as_raw_fd(), &mut driver_info) } {
            return Err(PciOpenError::IoctlError {
                name: "get_driver_info".to_string(),
                id: device_id,
                source: errorno,
            });
        }

        let arch = Arch::try_from(&device_info.output).map_err(|asic_id| {
            PciOpenError::UnrecognizedDeviceId {
                pci_id: device_id,
                device_id: asic_id,
            }
        })?;

        let max_dma_buf_size_log2 = device_info.output.max_dma_buf_size_log2;

        let pci_bus = device_info.output.bus_dev_fn >> 8;
        let slot = ((device_info.output.bus_dev_fn) >> 3) & 0x1f; // The definition of PCI_SLOT from include/uapi/linux/pci.h
        let pci_function = (device_info.output.bus_dev_fn) & 0x7; // The definition of PCI_FUNC from include/uapi/linux/pci.h
        let pci_domain = device_info.output.pci_domain;

        let config_space = std::fs::OpenOptions::new()
            .read(true)
            .write(false)
            .open(format!(
                "/sys/bus/pci/devices/{pci_domain:04x}:{pci_bus:02x}:{slot:02x}.{pci_function:01x}/config"
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
            },

            read_checking_enabled: true,
            read_checking_addr: if arch.is_blackhole() {
                kmdif::BH_NOC_NODE_ID_OFFSET
            } else {
                kmdif::GS_WH_ARC_SCRATCH6_ADDR
            },

            next_dma_buf: 0,

            device_fd: fd,
            driver_version: driver_info.output.driver_version,

            config_space,

            max_dma_buf_size_log2,

            dma_buffer_mappings: vec![],
            dma_config: None,
            completion_flag_buffer: None,
            transfer_buffer: None,

            pci_bar: None,
        };

        // To avoid needing a warmup when performing the first dma access, try to allocate the
        // buffers now.
        device.allocate_transfer_buffers();

        // If/when the raw bar mapping is removed this will start failing
        device.map_bar()?;

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

        match result {
            Ok(rc) => match rc {
                0 => Ok(TlbAllocation {
                    id: data.output.id,
                    uc_mapping,
                    size,
                }),
                errno => Err(PciError::IoctlError(nix::errno::Errno::from_i32(errno))),
            },
            Err(errno) => Err(PciError::IoctlError(errno)),
        }
    }

    pub fn free_tlb(&self, alloc: TlbAllocation) -> Result<(), PciError> {
        let id = alloc.id;

        // Explicitly unmap, otherwise the ioctl will return EBUSY
        drop(alloc);

        let result = unsafe {
            ioctl::free_tlb(
                self.device_fd.as_raw_fd(),
                (&mut ioctl::FreeTlb {
                    input: ioctl::FreeTlbIn { id },
                    output: ioctl::FreeTlbOut {},
                }) as *mut _,
            )
        };

        match result {
            Ok(0) => Ok(()),
            Ok(rc) => Err(PciError::IoctlError(nix::errno::Errno::from_i32(rc))),
            Err(errno) => Err(PciError::IoctlError(errno)),
        }
    }

    pub fn configure_tlb(
        &self,
        alloc: &TlbAllocation,
        config: ioctl::NocTlbConfig,
    ) -> Result<(), PciError> {
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
            Ok(0) => Ok(()),
            Ok(rc) => Err(PciError::IoctlError(nix::errno::Errno::from_i32(rc))),
            Err(errno) => Err(PciError::IoctlError(errno)),
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

    pub fn setup_tlb(
        &mut self,
        index: &PossibleTlbAllocation,
        tlb: Tlb,
    ) -> Result<(u64, u64), PciError> {
        match index {
            PossibleTlbAllocation::Allocation(tlb_allocation) => {
                let config = ioctl::NocTlbConfig {
                    addr: tlb.local_offset & !(tlb_allocation.size - 1),
                    x_end: tlb.x_end as u16,
                    y_end: tlb.y_end as u16,
                    x_start: tlb.x_start as u16,
                    y_start: tlb.y_start as u16,
                    noc: tlb.noc_sel,
                    mcast: tlb.mcast as u8,
                    ordering: tlb.ordering.into(),
                    linked: tlb.linked as u8,
                    static_vc: tlb.static_vc,
                    ..Default::default()
                };
                self.configure_tlb(tlb_allocation, config)?;

                // The tlb addr selects the upper bits of the final noc addrress
                // therefore the lower bits are the offset into the tlb itself, selected by the lower bits of our size.
                let tlb_offset = tlb.local_offset & (tlb_allocation.size - 1);

                Ok((
                    tlb_offset,
                    // The size must be shrunk by the offset into the tlb of our configured address
                    tlb_allocation.size - tlb_offset,
                ))
            }
            PossibleTlbAllocation::Hardcoded(index) => tlb::setup_tlb(self, *index, tlb),
            PossibleTlbAllocation::NoAllocation => {
                todo!("Need all fallback implementation if a tlb was not otherwise selected")
            }
        }
    }

    pub fn get_tlb(&self, index: &PossibleTlbAllocation) -> Result<Tlb, PciError> {
        let index = match index {
            PossibleTlbAllocation::Allocation(tlb_allocation) => tlb_allocation.id,
            PossibleTlbAllocation::Hardcoded(index) => *index,
            PossibleTlbAllocation::NoAllocation => {
                todo!("Need all fallback implementation if a tlb was not otherwise selected")
            }
        };

        tlb::get_tlb(self, index)
    }

    pub fn noc_write(
        &mut self,
        index: &PossibleTlbAllocation,
        mut tlb: Tlb,
        data: &[u8],
    ) -> Result<(), PciError> {
        let mut written = 0;
        let addr = tlb.local_offset;
        while written < data.len() {
            let (offset, size) = self.setup_tlb(index, tlb.clone())?;

            let remaining_data = &data[written..];
            let chunk = &remaining_data[..(size as usize).min(remaining_data.len())];

            match index {
                PossibleTlbAllocation::Allocation(tlb_allocation) => unsafe {
                    Self::memcpy_to_device(
                        (tlb_allocation.uc_mapping.as_ptr() as *mut u8).byte_add(offset as usize),
                        chunk,
                    );
                },
                PossibleTlbAllocation::Hardcoded(_index) => {
                    self.write_block(offset as u32, chunk)?;
                }
                PossibleTlbAllocation::NoAllocation => todo!(),
            }

            written += chunk.len();

            tlb.local_offset = addr + written as u64;
        }

        Ok(())
    }

    pub fn noc_read(
        &mut self,
        index: &PossibleTlbAllocation,
        mut tlb: Tlb,
        data: &mut [u8],
    ) -> Result<(), PciError> {
        let mut read = 0;
        let addr = tlb.local_offset;
        while read < data.len() {
            let (offset, size) = self.setup_tlb(index, tlb.clone())?;

            let remaining_buffer = &mut data[read..];
            let chunk_len = (size as usize).min(remaining_buffer.len());
            let chunk = &mut remaining_buffer[..chunk_len];

            match index {
                PossibleTlbAllocation::Allocation(tlb_allocation) => unsafe {
                    Self::memcpy_from_device(
                        chunk,
                        (tlb_allocation.uc_mapping.as_ptr() as *mut u8).byte_add(offset as usize),
                    );
                },
                PossibleTlbAllocation::Hardcoded(_index) => {
                    self.read_block(offset as u32, chunk)?;
                }
                PossibleTlbAllocation::NoAllocation => todo!(),
            }

            read += chunk.len();

            tlb.local_offset = addr + read as u64;
        }

        Ok(())
    }

    pub fn noc_write32(
        &mut self,
        tlb_index: &PossibleTlbAllocation,
        tlb: Tlb,
        data: u32,
    ) -> Result<(), PciError> {
        let (offset, size) = self.setup_tlb(tlb_index, tlb)?;
        assert!(
            size >= 4,
            "We have hit the the unlikely case of size being less than 4 bytes; this should not be possible"
        );
        match tlb_index {
            PossibleTlbAllocation::Allocation(tlb_allocation) => self.write32_no_translation(
                unsafe {
                    (tlb_allocation.uc_mapping.as_ptr() as *mut u8).byte_add(offset as usize)
                        as usize
                },
                data,
            ),
            PossibleTlbAllocation::Hardcoded(_) => self.write32(offset as u32, data),
            PossibleTlbAllocation::NoAllocation => todo!(),
        }
    }

    pub fn noc_read32(
        &mut self,
        tlb_index: &PossibleTlbAllocation,
        tlb: Tlb,
    ) -> Result<u32, PciError> {
        let (offset, size) = self.setup_tlb(tlb_index, tlb)?;
        assert!(
            size >= 4,
            "We have hit the the unlikely case of size being less than 4 bytes; this should not be possible"
        );
        match tlb_index {
            PossibleTlbAllocation::Allocation(tlb_allocation) => {
                self.read32_no_translation(unsafe {
                    (tlb_allocation.uc_mapping.as_ptr() as *mut u8).byte_add(offset as usize)
                        as usize
                })
            }
            PossibleTlbAllocation::Hardcoded(_) => self.read32(offset as u32),
            PossibleTlbAllocation::NoAllocation => todo!(),
        }
    }
}

#[derive(Debug, Default, PartialEq)]
pub struct DriverVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub quirk: Option<String>,
}

impl PartialOrd for DriverVersion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match self.major.cmp(&other.major) {
            std::cmp::Ordering::Equal => match self.minor.cmp(&other.minor) {
                std::cmp::Ordering::Equal => match self.patch.cmp(&other.patch) {
                    std::cmp::Ordering::Equal => {
                        if self.quirk == other.quirk {
                            Some(std::cmp::Ordering::Equal)
                        } else {
                            None
                        }
                    }
                    other => Some(other),
                },
                other => Some(other),
            },
            other => Some(other),
        }
    }
}

fn driver_version_parse(version: &str) -> DriverVersion {
    let mut driver_version = DriverVersion::default();
    let version = version.trim();

    let version = if let Some((a, b)) = version.split_once('-') {
        driver_version.quirk = Some(b.to_string());

        a
    } else {
        version
    };

    let mut it = version.splitn(3, '.');
    if let Some(v) = it.next() {
        if let Ok(v) = v.parse() {
            driver_version.major = v;
        }
    }
    if let Some(v) = it.next() {
        if let Ok(v) = v.parse() {
            driver_version.minor = v;
        }
    }
    if let Some(v) = it.next() {
        if let Ok(v) = v.parse() {
            driver_version.patch = v;
        }
    }

    driver_version
}

pub fn get_version() -> Option<DriverVersion> {
    std::fs::read_to_string("/sys/module/tenstorrent/version")
        .ok()
        .map(|version| driver_version_parse(&version))
}
#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_get_version() {
        assert_eq!(
            DriverVersion::default(),
            DriverVersion {
                major: 0,
                minor: 0,
                patch: 0,
                quirk: None
            },
            "default driver version did not match expected values"
        );
        assert_eq!(
            driver_version_parse("1"),
            DriverVersion {
                major: 1,
                ..Default::default()
            }
        );
        assert_eq!(
            driver_version_parse("1.33"),
            DriverVersion {
                major: 1,
                minor: 33,
                ..Default::default()
            }
        );
        assert_eq!(
            driver_version_parse("1.33.5"),
            DriverVersion {
                major: 1,
                minor: 33,
                patch: 5,
                ..Default::default()
            }
        );
        assert_eq!(
            driver_version_parse("1.33.5-quirk"),
            DriverVersion {
                major: 1,
                minor: 33,
                patch: 5,
                quirk: Some("quirk".to_string())
            }
        );
        assert_eq!(
            driver_version_parse("1.33-quirk"),
            DriverVersion {
                major: 1,
                minor: 33,
                quirk: Some("quirk".to_string()),
                ..Default::default()
            }
        );
        assert_eq!(
            driver_version_parse("1-quirk"),
            DriverVersion {
                major: 1,
                quirk: Some("quirk".to_string()),
                ..Default::default()
            }
        );
        assert_eq!(driver_version_parse("bad"), DriverVersion::default());
    }

    fn verify_noc(device: &mut PciDevice, tlb: PossibleTlbAllocation) {
        let node_info = match device.arch {
            Arch::Grayskull => {
                unimplemented!("Not currently supporting GS for this test\nTo support readback the noc node id from ARC");
            }
            Arch::Wormhole => (0, 10, 0xFFFB2002C),
            Arch::Blackhole => (8, 0, 0x0000000080050044),
        };

        let node_id = device
            .noc_read32(
                &tlb,
                Tlb {
                    x_end: node_info.0,
                    y_end: node_info.1,
                    noc_sel: 0,

                    local_offset: node_info.2,

                    ..Default::default()
                },
            )
            .unwrap();
        let x = node_id & 0x3f;
        let y = (node_id >> 6) & 0x3f;

        assert!(
            x == node_info.0 as u32 && y == node_info.1 as u32,
            "ARC node id didn't match expected ({x}, {y}) != ({}, {})",
            node_info.0,
            node_info.1
        );
    }

    #[test]
    #[cfg_attr(
        any(not(feature = "test_hardware"), feature = "test_grayskull"),
        ignore = "Requires hardware"
    )]
    fn ttkmd_allocate() {
        let mut device = PciDevice::scan()
            .into_iter()
            .map(PciDevice::open)
            .next()
            .expect("Expected to have access to 1 pci device")
            .unwrap();

        let tlb = device.allocate_tlb(1 << 20).unwrap();
        verify_noc(&mut device, PossibleTlbAllocation::Allocation(tlb));
    }

    #[test]
    #[cfg_attr(
        any(not(feature = "test_hardware"), feature = "test_grayskull"),
        ignore = "Requires hardware"
    )]
    fn ttkmd_no_allocate() {
        let mut device = PciDevice::scan()
            .into_iter()
            .map(PciDevice::open)
            .next()
            .expect("Expected to have access to 1 pci device")
            .unwrap();

        verify_noc(&mut device, PossibleTlbAllocation::Hardcoded(1));
    }
}
