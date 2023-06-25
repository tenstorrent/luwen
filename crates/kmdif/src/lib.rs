use std::{
    os::{fd::{AsRawFd, RawFd}, unix::prelude::FileTypeExt},
    sync::Arc,
};

mod error;
mod ioctl;
mod kmdif;
mod pci;

pub use error::{PciError, PciOpenError};
use ioctl::{
    query_mappings, AllocateDmaBuffer, GetDeviceInfo, GetDeviceInfoOut, Mapping, QueryMappings,
};

#[derive(Clone, Hash, Copy, Debug, PartialEq, Eq)]
pub enum Arch {
    Grayskull,
    Wormhole,
    Unknown(u16),
}

impl From<&GetDeviceInfoOut> for Arch {
    fn from(value: &GetDeviceInfoOut) -> Self {
        match value.device_id {
            0xfaca => Arch::Grayskull,
            0x401e => Arch::Wormhole,
            id => Arch::Unknown(id),
        }
    }
}

impl Arch {
    pub fn is_wormhole(&self) -> bool {
        match self {
            Arch::Wormhole => true,
            _ => false,
        }
    }

    pub fn is_grayskull(&self) -> bool {
        match self {
            Arch::Grayskull => true,
            _ => false,
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

    pub read_theshold: u32,
    pub write_theshold: u32,
}

pub struct PhysicalDevice {
    pub vendor_id: u16,
    pub device_id: u16,
    pub subsystem_vendor_id: u16,
    pub subsystem_id: u16,

    pub pci_bus: u16,
    pub pci_device: u16,
    pub pci_function: u16,

    pub bar_addr: u64,
    pub bar_size_bytes: u64,
}

pub struct PciDevice {
    pub id: usize,

    pub physical: PhysicalDevice,
    pub arch: Arch,

    pub read_checking_enabled: bool,

    next_dma_buf: usize,

    device_fd: std::fs::File,
    bar0_uc: memmap2::MmapMut,
    bar0_uc_size: usize,
    bar0_uc_offset: u64,

    bar0_wc: Option<memmap2::MmapMut>,
    bar0_wc_size: usize,

    config_space: std::fs::File,

    max_dma_buf_size_log2: u16,

    system_reg_mapping: Option<memmap2::MmapMut>,
    system_reg_mapping_size: usize,
    system_reg_start_offset: u32, // Registers >= this are system regs, use the mapping.
    system_reg_offset_adjust: u32, // This is the offset of the first reg in the system reg mapping.

    dma_buffer_mappings: Vec<Arc<DmaBuffer>>,
    completion_flag_buffer: DmaBuffer,
    transfer_buffer: DmaBuffer,

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
    allocate_dma_buf.input.requested_size = (size.min(1 << max_dma_buf_size_log2)).max(kmdif::getpagesize().unwrap() as u32);
    allocate_dma_buf.input.buf_index = buffer_index as u8;

    if let Err(err) = unsafe { ioctl::allocate_dma_buffer(device_fd, &mut allocate_dma_buf) } {
        panic!(
            "DMA buffer allocation on device {} failed ({} bytes) with error {err}",
            device_id, allocate_dma_buf.input.requested_size
        );
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

        let mut bar0_uc_mapping = Mapping::default();
        let mut bar0_wc_mapping = Mapping::default();
        let mut bar2_uc_mapping = Mapping::default();
        let mut bar2_wc_mapping = Mapping::default();

        for i in 0..mappings.input.output_mapping_count as usize {
            match kmdif::MappingId::from_u32(mappings.output.mappings[i].mapping_id) {
                kmdif::MappingId::Resource0Uc => {
                    bar0_uc_mapping = mappings.output.mappings[i];
                }
                kmdif::MappingId::Resource0Wc => {
                    bar0_wc_mapping = mappings.output.mappings[i];
                }
                kmdif::MappingId::Resource1Uc => {}
                kmdif::MappingId::Resource1Wc => {}
                kmdif::MappingId::Resource2Uc => {
                    bar2_uc_mapping = mappings.output.mappings[i];
                }
                kmdif::MappingId::Resource2Wc => {
                    bar2_wc_mapping = mappings.output.mappings[i];
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

        // if disable_wc
        // bar0_wc_mapping = 0;
        // bar0_wc_size = 0;
        // else
        let mut bar0_wc_size = 0;
        let mut bar0_wc = None;
        if bar0_wc_mapping.mapping_id == kmdif::MappingId::Resource0Wc.as_u32() {
            bar0_wc_size = bar0_wc_mapping
                .mapping_size
                .min(kmdif::GS_BAR0_WC_MAPPING_SIZE) as usize;
            let bar0_wc_map = unsafe {
                memmap2::MmapOptions::default()
                    .len(bar0_wc_size)
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
            bar0_uc_size = bar0_uc_mapping
                .mapping_size
                .saturating_sub(kmdif::GS_BAR0_WC_MAPPING_SIZE) as usize;
            bar0_uc_offset = kmdif::GS_BAR0_WC_MAPPING_SIZE;
        } else {
            // No WC mapping, map the entire BAR UC.
            bar0_uc_size = bar0_uc_mapping.mapping_size as usize;
            bar0_uc_offset = 0;
        }

        let bar0_uc = unsafe {
            memmap2::MmapOptions::default()
                .len(bar0_uc_size)
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
        if Arch::from(&device_info.output) == Arch::Wormhole {
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

        let pci_bus = device_info.output.bus_dev_fn >> 8;
        let pci_device = ((device_info.output.bus_dev_fn) >> 3) & 0x1f; // The definition of PCI_SLOT from include/uapi/linux/pci.h
        let pci_function = (device_info.output.bus_dev_fn) & 0x7; // The definition of PCI_FUNC from include/uapi/linux/pci.h

        let config_space = std::fs::OpenOptions::new()
            .read(true)
            .write(false)
            .open(format!(
                "/sys/bus/pci/devices/0000:{:02x}:{:02x}.{:01x}/config",
                pci_bus, pci_device, pci_function
            ));
        let config_space = match config_space {
            Ok(file) => file,
            Err(err) => {
                panic!("Failed to open config space for device {device_id} with error {err}");
            }
        };

        let mut device = PciDevice {
            id: device_id,
            arch: Arch::from(&device_info.output),

            physical: PhysicalDevice {
                vendor_id: device_info.output.vendor_id,
                device_id: device_info.output.device_id,
                subsystem_vendor_id: device_info.output.subsystem_vendor_id,
                subsystem_id: device_info.output.subsystem_id,
                pci_bus,
                pci_device,
                pci_function,
                bar_addr: pci::read_bar0_base(&config_space),
                bar_size_bytes: bar0_uc_mapping.mapping_size,
            },

            read_checking_enabled: true,

            next_dma_buf: 0,

            device_fd: fd,

            bar0_uc: bar0_uc,
            bar0_uc_size: bar0_uc_size,
            bar0_uc_offset: bar0_uc_offset,

            bar0_wc: bar0_wc,
            bar0_wc_size: bar0_wc_size,

            config_space,

            max_dma_buf_size_log2,

            system_reg_mapping,
            system_reg_mapping_size,
            system_reg_start_offset,
            system_reg_offset_adjust,

            dma_buffer_mappings: vec![],
            // Note(drosen): These are being allocated immidatly after this struct, they are created using unrelated apis.
            //               But to avoid issues fi the actual allocations run into errors we are allocating anon buffers here.
            completion_flag_buffer: DmaBuffer {
                buffer: memmap2::MmapMut::map_anon(0).map_err(|err| {
                    PciOpenError::FakeMmapFailed {
                        buffer: "completion_flag".to_string(),
                        device_id: device_id,
                        source: err,
                    }
                })?,
                physical_address: 0,
                size: 0,
            },
            transfer_buffer: DmaBuffer {
                buffer: memmap2::MmapMut::map_anon(0).map_err(|err| {
                    PciOpenError::FakeMmapFailed {
                        buffer: "transfer".to_string(),
                        device_id: device_id,
                        source: err,
                    }
                })?,
                physical_address: 0,
                size: 0,
            },

            dma_config: None,
        };

        device.completion_flag_buffer =
            device.allocate_dma_buffer(std::mem::size_of::<u64>() as u32)?;
        device.transfer_buffer = device.allocate_dma_buffer_range(
            kmdif::getpagesize().unwrap() as u32,
            kmdif::MAX_DMA_BYTES,
        )?;

        Ok(device)
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
                self.id as usize,
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

    pub fn scan() -> Vec<usize> {
        let mut output = std::fs::read_dir("/dev/tenstorrent")
            .unwrap()
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
