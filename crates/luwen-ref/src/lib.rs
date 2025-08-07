// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{hash_map::Entry, HashMap},
    sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard},
};

use error::LuwenError;
use luwen_core::Arch;
use luwen_if::{FnDriver, FnOptions};
use ttkmd_if::{PciError, PossibleTlbAllocation};

mod detect;
pub mod error;
mod wormhole;

use wormhole::ethernet::{self, EthCommCoord};

pub use detect::{
    detect_chips, detect_chips_fallible, detect_chips_silent, detect_local_chips, start_detect,
};
pub use ttkmd_if::{DmaBuffer, DmaConfig, PciDevice, Tlb};

#[derive(Clone)]
pub struct ExtendedPciDeviceWrapper {
    inner: Arc<RwLock<ExtendedPciDevice>>,
}

impl ExtendedPciDeviceWrapper {
    pub fn borrow_mut(&self) -> RwLockWriteGuard<'_, ExtendedPciDevice> {
        self.inner.as_ref().write().unwrap()
    }

    pub fn borrow(&self) -> RwLockReadGuard<'_, ExtendedPciDevice> {
        self.inner.as_ref().read().unwrap()
    }
}

pub struct ExtendedPciDevice {
    pub device: PciDevice,

    pub harvested_rows: u32,
    pub grid_size_x: u8,
    pub grid_size_y: u8,

    pub eth_x: u8,
    pub eth_y: u8,
    pub command_q_addr: u32,
    pub fake_block: bool,

    pub default_tlb: PossibleTlbAllocation,

    pub ethernet_dma_buffer: HashMap<(u8, u8), DmaBuffer>,
}

impl ExtendedPciDevice {
    pub fn open(pci_interface: usize) -> Result<ExtendedPciDeviceWrapper, ttkmd_if::PciError> {
        let device = PciDevice::open(pci_interface)?;

        let (grid_size_x, grid_size_y) = match device.arch {
            luwen_core::Arch::Grayskull => (13, 12),
            luwen_core::Arch::Wormhole => (10, 12),
            luwen_core::Arch::Blackhole => (17, 12),
        };

        let default_tlb;

        // Driver API 2+ has TLB allocation APIs supporting WH & BH.
        if device.arch != Arch::Grayskull && device.driver_version >= 2 {
            let size = match device.arch {
                Arch::Wormhole => 1 << 24,  // 16 MiB
                Arch::Blackhole => 1 << 21, // 2 MiB
                _ => {
                    return Err(PciError::TlbAllocationError(
                        "Unsupported architecture for TLB allocation".to_string(),
                    ))
                }
            };

            if let Ok(tlb) = device.allocate_tlb(size) {
                default_tlb = PossibleTlbAllocation::Allocation(tlb);
            } else {
                // Couldn't get a tlb... ideally at this point we would fallback to using a slower but useable read/write API
                // for now though, we will just fail
                return Err(PciError::TlbAllocationError(
                    "Failed to find a free tlb".to_string(),
                ));
            }
        } else {
            // Otherwise fallback to default behaviour of just taking a constant one
            default_tlb = PossibleTlbAllocation::Hardcoded(match device.arch {
                luwen_core::Arch::Grayskull | luwen_core::Arch::Wormhole => 184,
                luwen_core::Arch::Blackhole => 190,
            });
        }

        Ok(ExtendedPciDeviceWrapper {
            inner: Arc::new(RwLock::new(ExtendedPciDevice {
                harvested_rows: 0,
                grid_size_x,
                grid_size_y,
                eth_x: 4,
                eth_y: 6,
                command_q_addr: 0,
                fake_block: false,

                default_tlb,

                device,

                ethernet_dma_buffer: HashMap::with_capacity(16),
            })),
        })
    }

    pub fn read_block(&mut self, addr: u32, data: &mut [u8]) -> Result<(), PciError> {
        self.device.read_block(addr, data)
    }

    pub fn write_block(&mut self, addr: u32, data: &[u8]) -> Result<(), PciError> {
        self.device.write_block(addr, data)
    }
}

pub fn comms_callback(
    ud: &ExtendedPciDeviceWrapper,
    op: FnOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    Ok(comms_callback_inner(ud, op)?)
}

pub fn comms_callback_inner(
    ud: &ExtendedPciDeviceWrapper,
    op: FnOptions,
) -> Result<(), LuwenError> {
    match op {
        FnOptions::Driver(op) => match op {
            FnDriver::DeviceInfo(info) => {
                let borrow = ud.borrow();
                if !info.is_null() {
                    unsafe {
                        *info = Some(luwen_if::DeviceInfo {
                            bus: borrow.device.physical.pci_bus,
                            slot: borrow.device.physical.slot,
                            function: borrow.device.physical.pci_function,
                            domain: borrow.device.physical.pci_domain,

                            interface_id: borrow.device.id as u32,

                            vendor: borrow.device.physical.vendor_id,
                            device_id: borrow.device.physical.device_id,
                            board_id: borrow.device.physical.subsystem_id,
                            bar_size: borrow.device.pci_bar.as_ref().map(|v| v.bar_size_bytes),
                        });
                    }
                }
            }
        },
        FnOptions::Axi(op) => match op {
            luwen_if::FnAxi::Read { addr, data, len } => {
                if len > 0 {
                    if len <= 4 {
                        let output = ud.borrow_mut().device.read32(addr)?;
                        let output = output.to_le_bytes();
                        unsafe {
                            data.copy_from_nonoverlapping(output.as_ptr(), len as usize);
                        }
                    } else {
                        unsafe {
                            ud.borrow_mut().read_block(
                                addr,
                                std::slice::from_raw_parts_mut(data, len as usize),
                            )?
                        };
                    }
                }
            }
            luwen_if::FnAxi::Write { addr, data, len } => {
                if len > 0 {
                    // Assuming here that u32 is our fundamental unit of transfer
                    if len <= 4 {
                        let to_write = if len == 4 {
                            let slice = unsafe { std::slice::from_raw_parts(data, len as usize) };
                            u32::from_le_bytes(slice.try_into().unwrap())
                        } else {
                            // We are reading less than a u32, so we need to read the existing value first
                            // then writeback the new value with the lower len bytes replaced
                            let value = ud.borrow_mut().device.read32(addr)?;
                            let mut value = value.to_le_bytes();
                            unsafe {
                                value
                                    .as_mut_ptr()
                                    .copy_from_nonoverlapping(data, len as usize);
                            }

                            u32::from_le_bytes(value)
                        };

                        ud.borrow_mut().device.write32(addr, to_write)?;
                    } else {
                        unsafe {
                            ud.borrow_mut()
                                .write_block(addr, std::slice::from_raw_parts(data, len as usize))?
                        };
                    }
                }
            }
        },
        FnOptions::Noc(op) => match op {
            luwen_if::FnNoc::Read {
                noc_id,
                x,
                y,
                addr,
                data,
                len,
            } => {
                let mut reader = ud.borrow_mut();
                let reader: &mut ExtendedPciDevice = &mut reader;

                reader.device.noc_read(
                    &reader.default_tlb,
                    Tlb {
                        local_offset: addr,
                        x_end: x as u8,
                        y_end: y as u8,
                        noc_sel: noc_id,
                        mcast: false,
                        ..Default::default()
                    },
                    unsafe { std::slice::from_raw_parts_mut(data, len as usize) },
                )?;
            }
            luwen_if::FnNoc::Write {
                noc_id,
                x,
                y,
                addr,
                data,
                len,
            } => {
                let mut writer = ud.borrow_mut();
                let writer: &mut ExtendedPciDevice = &mut writer;

                writer.device.noc_write(
                    &writer.default_tlb,
                    Tlb {
                        local_offset: addr,
                        x_end: x as u8,
                        y_end: y as u8,
                        noc_sel: noc_id,
                        mcast: false,
                        ..Default::default()
                    },
                    unsafe { std::slice::from_raw_parts(data, len as usize) },
                )?;
            }
            luwen_if::FnNoc::Broadcast {
                noc_id,
                addr,
                data,
                len,
            } => {
                let mut writer = ud.borrow_mut();
                let writer: &mut ExtendedPciDevice = &mut writer;

                let (x_start, y_start) = match writer.device.arch {
                    luwen_core::Arch::Grayskull => (0, 0),
                    luwen_core::Arch::Wormhole => (1, 0),
                    luwen_core::Arch::Blackhole => (0, 1),
                };

                writer.device.noc_write(
                    &writer.default_tlb,
                    Tlb {
                        local_offset: addr,
                        x_start,
                        y_start,
                        x_end: writer.grid_size_x - 1,
                        y_end: writer.grid_size_y - 1,
                        noc_sel: noc_id,
                        mcast: true,
                        ..Default::default()
                    },
                    unsafe { std::slice::from_raw_parts(data, len as usize) },
                )?;
            }
            luwen_if::FnNoc::Multicast {
                noc_id,
                start_x,
                start_y,
                end_x,
                end_y,
                addr,
                data,
                len,
            } => {
                let mut writer = ud.borrow_mut();
                let writer: &mut ExtendedPciDevice = &mut writer;

                let (min_start_x, min_start_y) = match writer.device.arch {
                    luwen_core::Arch::Grayskull => (0, 0),
                    luwen_core::Arch::Wormhole => (1, 0),
                    luwen_core::Arch::Blackhole => (0, 1),
                };

                let (start_x, start_y) = (start_x.max(min_start_x), start_y.max(min_start_y));

                writer.device.noc_write(
                    &writer.default_tlb,
                    Tlb {
                        local_offset: addr,
                        x_start: start_x,
                        y_start: start_y,
                        x_end: end_x,
                        y_end: end_y,
                        noc_sel: noc_id,
                        mcast: true,
                        ..Default::default()
                    },
                    unsafe { std::slice::from_raw_parts(data, len as usize) },
                )?;
            }
        },
        FnOptions::Eth(op) => match op.rw {
            luwen_if::FnNoc::Read {
                noc_id,
                x,
                y,
                addr,
                data,
                len,
            } => {
                let mut borrow = ud.borrow_mut();
                let borrow: &mut ExtendedPciDevice = &mut borrow;

                let eth_x = borrow.eth_x;
                let eth_y = borrow.eth_y;

                let command_q_addr = borrow.device.noc_read32(
                    &borrow.default_tlb,
                    Tlb {
                        local_offset: 0x170,
                        noc_sel: 0,
                        x_end: eth_x,
                        y_end: eth_y,
                        ..Default::default()
                    },
                )?;
                let fake_block = borrow.fake_block;

                let default_tlb = &mut borrow.default_tlb;
                let read32 = |borrow: &mut PciDevice, addr| {
                    borrow.noc_read32(
                        default_tlb,
                        Tlb {
                            local_offset: addr,
                            noc_sel: 0,
                            x_end: eth_x,
                            y_end: eth_y,
                            ..Default::default()
                        },
                    )
                };

                let write32 = |borrow: &mut PciDevice, addr, data| {
                    borrow.noc_write32(
                        default_tlb,
                        Tlb {
                            local_offset: addr,
                            noc_sel: 0,
                            x_end: eth_x,
                            y_end: eth_y,
                            ..Default::default()
                        },
                        data,
                    )
                };

                let dma_buffer = {
                    let key = (eth_x, eth_y);
                    if let Entry::Vacant(e) = borrow.ethernet_dma_buffer.entry(key) {
                        // 1 MB buffer
                        e.insert(borrow.device.allocate_dma_buffer(1 << 20)?);
                    }

                    // SAFETY: Can never get here without first inserting something into the hashmap
                    unsafe { borrow.ethernet_dma_buffer.get_mut(&key).unwrap_unchecked() }
                };

                ethernet::fixup_queues(&mut borrow.device, read32, write32, command_q_addr)?;

                if len <= 4 {
                    let value = ethernet::eth_read32(
                        &mut borrow.device,
                        read32,
                        write32,
                        command_q_addr,
                        EthCommCoord {
                            coord: op.addr,
                            noc_id,
                            noc_x: x as u8,
                            noc_y: y as u8,
                            offset: addr,
                        },
                        std::time::Duration::from_secs(5 * 60),
                    )?;

                    let sl = unsafe { std::slice::from_raw_parts_mut(data, len as usize) };
                    let vl = value.to_le_bytes();

                    for (s, v) in sl.iter_mut().zip(vl.iter()) {
                        *s = *v;
                    }
                } else {
                    ethernet::block_read(
                        &mut borrow.device,
                        read32,
                        write32,
                        dma_buffer,
                        command_q_addr,
                        std::time::Duration::from_secs(5 * 60),
                        fake_block,
                        EthCommCoord {
                            coord: op.addr,
                            noc_id,
                            noc_x: x as u8,
                            noc_y: y as u8,
                            offset: addr,
                        },
                        unsafe { std::slice::from_raw_parts_mut(data, len as usize) },
                    )?;
                }
            }
            luwen_if::FnNoc::Write {
                noc_id,
                x,
                y,
                addr,
                data,
                len,
            } => {
                let mut borrow = ud.borrow_mut();
                let borrow: &mut ExtendedPciDevice = &mut borrow;

                let eth_x = borrow.eth_x;
                let eth_y = borrow.eth_y;

                let command_q_addr = borrow.device.noc_read32(
                    &borrow.default_tlb,
                    Tlb {
                        local_offset: 0x170,
                        noc_sel: 0,
                        x_end: eth_x,
                        y_end: eth_y,
                        ..Default::default()
                    },
                )?;
                let fake_block = borrow.fake_block;

                let default_tlb = &borrow.default_tlb;
                let read32 = |borrow: &mut PciDevice, addr| {
                    borrow.noc_read32(
                        default_tlb,
                        Tlb {
                            local_offset: addr,
                            noc_sel: 0,
                            x_end: eth_x,
                            y_end: eth_y,
                            ..Default::default()
                        },
                    )
                };

                let write32 = |borrow: &mut PciDevice, addr, data| {
                    borrow.noc_write32(
                        default_tlb,
                        Tlb {
                            local_offset: addr,
                            noc_sel: 0,
                            x_end: eth_x,
                            y_end: eth_y,
                            ..Default::default()
                        },
                        data,
                    )
                };

                let dma_buffer = {
                    let key = (eth_x, eth_y);
                    if let Entry::Vacant(e) = borrow.ethernet_dma_buffer.entry(key) {
                        // 1 MB buffer
                        e.insert(borrow.device.allocate_dma_buffer(1 << 20)?);
                    }

                    // SAFETY: Can never get here without first inserting something into the hashmap
                    unsafe { borrow.ethernet_dma_buffer.get_mut(&key).unwrap_unchecked() }
                };

                ethernet::fixup_queues(&mut borrow.device, read32, write32, command_q_addr)?;

                if len <= 4 {
                    let sl = unsafe { std::slice::from_raw_parts(data, len as usize) };
                    let mut value = 0u32;
                    for s in sl.iter().rev() {
                        value <<= 8;
                        value |= *s as u32;
                    }

                    ethernet::eth_write32(
                        &mut borrow.device,
                        read32,
                        write32,
                        command_q_addr,
                        EthCommCoord {
                            coord: op.addr,
                            noc_id,
                            noc_x: x as u8,
                            noc_y: y as u8,
                            offset: addr,
                        },
                        std::time::Duration::from_secs(5 * 60),
                        value,
                    )?;
                } else {
                    ethernet::block_write(
                        &mut borrow.device,
                        read32,
                        write32,
                        dma_buffer,
                        command_q_addr,
                        std::time::Duration::from_secs(5 * 60),
                        fake_block,
                        EthCommCoord {
                            coord: op.addr,
                            noc_id,
                            noc_x: x as u8,
                            noc_y: y as u8,
                            offset: addr,
                        },
                        unsafe { std::slice::from_raw_parts(data, len as usize) },
                    )?;
                }
            }
            luwen_if::FnNoc::Broadcast {
                noc_id,
                addr,
                data,
                len,
            } => {
                todo!("Tried to do an ethernet broadcast which is not supported, noc_id: {}, addr: {:#x}, data: {:p}, len: {:x}", noc_id, addr, data, len);
            }
            luwen_if::FnNoc::Multicast {
                noc_id,
                start_x,
                start_y,
                end_x,
                end_y,
                addr,
                data,
                len,
            } => {
                todo!("Tried to do an ethernet multicast which is not supported, noc_id: {}, start: ({}, {}), end: ({}, {}), addr: {:#x}, data: {:p}, len: {:x}", noc_id, start_x, start_y, end_x, end_y, addr, data, len);
            }
        },
    }

    Ok(())
}

pub fn open(interface_id: usize) -> Result<luwen_if::chip::Chip, LuwenError> {
    let ud = ExtendedPciDevice::open(interface_id)?;

    let arch = ud.borrow().device.arch;

    Ok(luwen_if::chip::Chip::open(
        arch,
        luwen_if::CallbackStorage::new(comms_callback, ud.clone()),
    )?)
}
