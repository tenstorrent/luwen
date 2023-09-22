use std::{
    collections::HashMap,
    sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard},
};

use kmdif::{DmaBuffer, PciError, Tlb};
use luwen_if::{FnDriver, FnOptions};

mod detect;
pub mod error;
mod wormhole;

use wormhole::ethernet::{self, EthCommCoord};

pub use detect::detect_chips;
pub use kmdif::PciDevice;

#[derive(Clone)]
pub struct ExtendedPciDeviceWrapper {
    inner: Arc<RwLock<ExtendedPciDevice>>,
}

impl ExtendedPciDeviceWrapper {
    pub fn borrow_mut(&self) -> RwLockWriteGuard<ExtendedPciDevice> {
        self.inner.as_ref().write().unwrap()
    }

    pub fn borrow(&self) -> RwLockReadGuard<ExtendedPciDevice> {
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

    pub ethernet_dma_buffer: HashMap<(u8, u8), DmaBuffer>,
}

impl ExtendedPciDevice {
    pub fn open(pci_interface: usize) -> Result<ExtendedPciDeviceWrapper, kmdif::PciOpenError> {
        let device = PciDevice::open(pci_interface)?;
        Ok(ExtendedPciDeviceWrapper {
            inner: Arc::new(RwLock::new(ExtendedPciDevice {
                device,
                harvested_rows: 0,
                grid_size_x: 0,
                grid_size_y: 0,
                eth_x: 4,
                eth_y: 6,
                command_q_addr: 0,
                fake_block: false,

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

fn noc_write32(
    device: &mut PciDevice,
    noc_id: u8,
    x: u8,
    y: u8,
    addr: u32,
    data: u32,
) -> Result<(), PciError> {
    let (bar_addr, _slice_len) = kmdif::tlb::setup_tlb(
        device,
        170,
        Tlb {
            local_offset: addr as u64,
            x_end: x as u8,
            y_end: y as u8,
            noc_sel: noc_id,
            mcast: false,
            ..Default::default()
        },
    )
    .unwrap();

    device.write_block(bar_addr as u32, data.to_le_bytes().as_slice())
}

fn noc_read32(
    device: &mut PciDevice,
    noc_id: u8,
    x: u8,
    y: u8,
    addr: u32,
) -> Result<u32, PciError> {
    let (bar_addr, _slice_len) = kmdif::tlb::setup_tlb(
        device,
        170,
        Tlb {
            local_offset: addr as u64,
            x_end: x as u8,
            y_end: y as u8,
            noc_sel: noc_id,
            mcast: false,
            ..Default::default()
        },
    )
    .unwrap();

    let mut data = [0u8; 4];
    device.read_block(bar_addr as u32, &mut data)?;
    Ok(u32::from_le_bytes(data))
}

pub fn comms_callback(ud: &ExtendedPciDeviceWrapper, op: FnOptions) {
    if let Err(output) = comms_callback_inner(ud, op) {
        panic!("Error: {}", output);
    }
}

pub fn comms_callback_inner(ud: &ExtendedPciDeviceWrapper, op: FnOptions) -> Result<(), PciError> {
    match op {
        FnOptions::Driver(op) => match op {
            FnDriver::DeviceInfo(info) => {
                let borrow = ud.borrow();
                if !info.is_null() {
                    unsafe {
                        *info = Some(luwen_if::DeviceInfo {
                            bus: borrow.device.physical.pci_bus,
                            device: borrow.device.physical.pci_device,
                            function: borrow.device.physical.pci_function,
                            domain: borrow.device.physical.pci_domain,

                            interface_id: borrow.device.id as u32,

                            vendor: borrow.device.physical.vendor_id,
                            device_id: borrow.device.physical.device_id,
                            bar_size: borrow.device.physical.bar_size_bytes,
                        });
                    }
                }
            }
        },
        FnOptions::Axi(op) => match op {
            luwen_if::FnAxi::Read { addr, data, len } => {
                unsafe {
                    ud.borrow_mut()
                        .read_block(addr, std::slice::from_raw_parts_mut(data, len as usize))?
                };
            }
            luwen_if::FnAxi::Write { addr, data, len } => {
                unsafe {
                    ud.borrow_mut()
                        .write_block(addr, std::slice::from_raw_parts(data, len as usize))?
                };
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

                let mut read = 0;
                while read < len {
                    let (bar_addr, slice_len) = kmdif::tlb::setup_tlb(
                        &mut reader.device,
                        168,
                        Tlb {
                            local_offset: addr + read,
                            x_end: x as u8,
                            y_end: y as u8,
                            noc_sel: noc_id,
                            mcast: false,
                            ..Default::default()
                        },
                    )?;

                    let to_read = std::cmp::min(slice_len, len.saturating_sub(read));
                    reader.read_block(bar_addr as u32, unsafe {
                        std::slice::from_raw_parts_mut(data.add(read as usize), to_read as usize)
                    })?;

                    read += to_read;
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
                let mut writer = ud.borrow_mut();

                let mut written = 0;
                while written < len {
                    let (bar_addr, slice_len) = kmdif::tlb::setup_tlb(
                        &mut writer.device,
                        184,
                        Tlb {
                            local_offset: addr + written,
                            x_end: x as u8,
                            y_end: y as u8,
                            noc_sel: noc_id,
                            mcast: false,
                            ..Default::default()
                        },
                    )?;

                    let to_write = std::cmp::min(slice_len, len.saturating_sub(written));
                    writer.write_block(bar_addr as u32, unsafe {
                        std::slice::from_raw_parts(data.add(written as usize), to_write as usize)
                    })?;

                    written += to_write;
                }
            }
            luwen_if::FnNoc::Broadcast {
                noc_id,
                addr,
                data,
                len,
            } => {
                let mut writer = ud.borrow_mut();

                let mut written = 0;
                while written < len {
                    let (bar_addr, slice_len) = kmdif::tlb::setup_tlb(
                        &mut writer.device,
                        170,
                        Tlb {
                            local_offset: addr + written,
                            x_start: 0,
                            y_start: 0,
                            x_end: ud.borrow().grid_size_x,
                            y_end: ud.borrow().grid_size_y,
                            noc_sel: noc_id,
                            mcast: true,
                            ..Default::default()
                        },
                    )
                    .unwrap();

                    let to_write = std::cmp::min(slice_len, len.saturating_sub(written));
                    writer.write_block(bar_addr as u32, unsafe {
                        std::slice::from_raw_parts(data.add(written as usize), to_write as usize)
                    })?;

                    written += to_write;
                }
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
                let eth_x = ud.borrow().eth_x;
                let eth_y = ud.borrow().eth_y;

                let mut borrow = ud.borrow_mut();
                let borrow: &mut ExtendedPciDevice = &mut borrow;

                let command_q_addr = noc_read32(&mut borrow.device, 0, eth_x, eth_y, 0x170)?;
                let fake_block = borrow.fake_block;

                let read32 = |borrow: &mut _, addr| noc_read32(borrow, 0, eth_x, eth_y, addr);

                let write32 =
                    |borrow: &mut _, addr, data| noc_write32(borrow, 0, eth_x, eth_y, addr, data);

                let dma_buffer = {
                    let key = (eth_x, eth_y);
                    if !borrow.ethernet_dma_buffer.contains_key(&key) {
                        // 1 MB buffer
                        borrow
                            .ethernet_dma_buffer
                            .insert(key, borrow.device.allocate_dma_buffer(1 << 20)?);
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
                    let vl = value.to_ne_bytes();

                    for (s, v) in sl.iter_mut().rev().zip(vl.iter().rev()) {
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
                let eth_x = ud.borrow().eth_x;
                let eth_y = ud.borrow().eth_y;

                let command_q_addr =
                    noc_read32(&mut ud.borrow_mut().device, 0, eth_x, eth_y, 0x170)?;
                let fake_block = ud.borrow().fake_block;

                let mut borrow = ud.borrow_mut();
                let borrow: &mut ExtendedPciDevice = &mut borrow;

                let read32 = |borrow: &mut _, addr| noc_read32(borrow, 0, eth_x, eth_y, addr);

                let write32 =
                    |borrow: &mut _, addr, data| noc_write32(borrow, 0, eth_x, eth_y, addr, data);

                let dma_buffer = {
                    let key = (eth_x, eth_y);
                    if !borrow.ethernet_dma_buffer.contains_key(&key) {
                        // 1 MB buffer
                        borrow
                            .ethernet_dma_buffer
                            .insert(key, borrow.device.allocate_dma_buffer(1 << 20)?);
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
        },
    }

    Ok(())
}
