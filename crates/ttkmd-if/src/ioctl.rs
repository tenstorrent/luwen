// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

const TENSTORRENT_IOCTL_MAGIC: usize = 0xFA;

use nix::request_code_none;

#[derive(Default)]
#[repr(C)]
pub struct GetDeviceInfoIn {
    pub output_size_bytes: u32,
}

#[derive(Default)]
#[repr(C)]
pub struct GetDeviceInfoOut {
    pub output_size_bytes: u32,
    pub vendor_id: u16,
    pub device_id: u16,
    pub subsystem_vendor_id: u16,
    pub subsystem_id: u16,
    pub bus_dev_fn: u16,            // [0:2] function, [3:7] device, [8:15] bus
    pub max_dma_buf_size_log2: u16, // Since 1.0
    pub pci_domain: u16,            // Since 1.23
}

#[derive(Default)]
#[repr(C)]
pub struct GetDeviceInfo {
    pub input: GetDeviceInfoIn,
    pub output: GetDeviceInfoOut,
}

nix::ioctl_readwrite_bad!(
    get_device_info,
    request_code_none!(TENSTORRENT_IOCTL_MAGIC, 0),
    GetDeviceInfo
);

#[derive(Debug, Default, Clone, Copy)]
#[repr(C)]
pub struct Mapping {
    pub mapping_id: u32,
    _reserved: u32,
    pub mapping_base: u64,
    pub mapping_size: u64,
}

#[derive(Debug, Default)]
#[repr(C)]
pub struct QueryMappingsIn {
    pub output_mapping_count: u32,
    _reserved: u32,
}

#[derive(Debug)]
#[repr(C)]
pub struct QueryMappingsOut<const N: usize> {
    pub mappings: [Mapping; N],
}

impl<const N: usize> Default for QueryMappingsOut<N> {
    fn default() -> Self {
        Self {
            mappings: [Mapping::default(); N],
        }
    }
}

#[derive(Debug)]
#[repr(C)]
pub struct QueryMappings<const N: usize> {
    pub input: QueryMappingsIn,
    pub output: QueryMappingsOut<N>,
}

impl<const N: usize> Default for QueryMappings<N> {
    fn default() -> Self {
        Self {
            input: QueryMappingsIn {
                output_mapping_count: N as u32,
                ..Default::default()
            },
            output: QueryMappingsOut::<N>::default(),
        }
    }
}

/// # Safety
///
/// You must make sure that data is a valid pointer and that the file descriptor is valid
pub unsafe fn query_mappings<const N: usize>(
    fd: nix::libc::c_int,
    data: *mut QueryMappings<N>,
) -> nix::Result<nix::libc::c_int> {
    nix::convert_ioctl_res!(nix::libc::ioctl(
        fd,
        request_code_none!(TENSTORRENT_IOCTL_MAGIC, 2) as nix::sys::ioctl::ioctl_num_type,
        data
    ))
}

#[derive(Default)]
#[repr(C)]
pub struct AllocateDmaBufferIn {
    pub requested_size: u32,
    pub buf_index: u8, // [0,TENSTORRENT_MAX_DMA_BUFS)
    _reserved0: [u8; 3],
    _reserved1: [u64; 2],
}

#[derive(Default)]
#[repr(C)]
pub struct AllocateDmaBufferOut {
    pub physical_address: u64,
    pub mapping_offset: u64,
    pub size: u32,
    _reserved0: u32,
    _reserved1: [u64; 2],
}

#[derive(Default)]
#[repr(C)]
pub struct AllocateDmaBuffer {
    pub input: AllocateDmaBufferIn,
    pub output: AllocateDmaBufferOut,
}

nix::ioctl_readwrite_bad!(
    allocate_dma_buffer,
    request_code_none!(TENSTORRENT_IOCTL_MAGIC, 3),
    AllocateDmaBuffer
);

#[derive(Default)]
#[repr(C)]
pub struct FreeDmaBufferIn;
#[derive(Default)]
#[repr(C)]
pub struct FreeDmaBufferOut;

#[derive(Default)]
#[repr(C)]
pub struct FreeDmaBuffer {
    pub input: FreeDmaBufferIn,
    pub output: FreeDmaBufferOut,
}

nix::ioctl_readwrite_bad!(
    free_dma_buffer,
    request_code_none!(TENSTORRENT_IOCTL_MAGIC, 4),
    FreeDmaBuffer
);

#[derive(Default)]
#[repr(C)]
pub struct GetDriverInfoIn {
    pub output_size_bytes: u32,
}

#[derive(Default)]
#[repr(C)]
pub struct GetDriverInfoOut {
    pub output_size_bytes: u32,
    pub driver_version: u32,
}

#[derive(Default)]
#[repr(C)]
pub struct GetDriverInfo {
    pub input: GetDriverInfoIn,
    pub output: GetDriverInfoOut,
}

nix::ioctl_readwrite_bad!(
    get_driver_info,
    request_code_none!(TENSTORRENT_IOCTL_MAGIC, 5),
    GetDriverInfo
);

pub const RESET_DEVICE_RESTORE_STATE: u32 = 0;
pub const RESET_DEVICE_RESET_PCIE_LINK: u32 = 1;
pub const RESET_DEVICE_RESET_CONFIG_WRITE: u32 = 2;

#[repr(C)]
pub struct ResetDeviceIn {
    pub output_size_bytes: u32,
    pub flags: u32,
}

impl Default for ResetDeviceIn {
    fn default() -> Self {
        Self {
            output_size_bytes: std::mem::size_of::<Self>() as u32,
            flags: 0,
        }
    }
}

#[derive(Default)]
#[repr(C)]
pub struct ResetDeviceOut {
    pub output_size_bytes: u32,
    pub result: u32,
}

#[derive(Default)]
#[repr(C)]
pub struct ResetDevice {
    pub input: ResetDeviceIn,
    pub output: ResetDeviceOut,
}

nix::ioctl_readwrite_bad!(
    reset_device,
    request_code_none!(TENSTORRENT_IOCTL_MAGIC, 6),
    ResetDevice
);
