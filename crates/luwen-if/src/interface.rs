// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::chip::{ChipInterface, eth_addr::EthAddr};

#[derive(Debug)]
pub enum FnNoc {
    Read {
        noc_id: u8,
        x: u32,
        y: u32,
        addr: u64,
        data: *mut u8,
        len: u64,
    },
    Write {
        noc_id: u8,
        x: u32,
        y: u32,
        addr: u64,
        data: *const u8,
        len: u64,
    },
    Broadcast {
        noc_id: u8,
        addr: u64,
        data: *const u8,
        len: u64,
    },
}

#[derive(Debug)]
pub struct FnRemote {
    pub addr: EthAddr,
    pub rw: FnNoc,
}

#[derive(Debug)]
pub enum FnAxi {
    Read {
        addr: u32,
        data: *mut u8,
        len: u32,
    },
    Write {
        addr: u32,
        data: *const u8,
        len: u32,
    },
}

#[derive(Clone, Debug)]
pub struct DeviceInfo {
    pub interface_id: u32,

    pub domain: u16,
    pub bus: u16,
    pub slot: u16,
    pub function: u16,

    pub vendor: u16,
    pub device_id: u16,
    pub bar_size: u64,
}

#[derive(Debug)]
pub enum FnDriver {
    DeviceInfo(*mut Option<DeviceInfo>),
}

#[derive(Debug)]
pub enum FnOptions {
    Driver(FnDriver),
    Axi(FnAxi),
    Noc(FnNoc),
    Eth(FnRemote),
}

#[derive(Clone)]
pub struct CallbackStorage<T: Clone + Send> {
    pub callback: fn(&T, FnOptions) -> Result<(), Box<dyn std::error::Error>>,
    pub user_data: T,
}

impl<T: Clone + Send> CallbackStorage<T> {
    pub fn new(
        callback: fn(&T, FnOptions) -> Result<(), Box<dyn std::error::Error>>,
        user_data: T,
    ) -> Self {
        Self {
            callback,
            user_data,
        }
    }
}

impl<T: Clone + Send + 'static> ChipInterface for CallbackStorage<T> {
    fn get_device_info(&self) -> Result<Option<DeviceInfo>, Box<dyn std::error::Error>> {
        let mut driver_info = None;
        (self.callback)(
            &self.user_data,
            FnOptions::Driver(FnDriver::DeviceInfo((&mut driver_info) as *mut _)),
        )?;

        Ok(driver_info)
    }

    fn axi_read(&self, addr: u32, data: &mut [u8]) -> Result<(), Box<dyn std::error::Error>> {
        (self.callback)(
            &self.user_data,
            FnOptions::Axi(FnAxi::Read {
                addr,
                data: data.as_mut_ptr(),
                len: data.len() as u32,
            }),
        )
    }

    fn axi_write(&self, addr: u32, data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        (self.callback)(
            &self.user_data,
            FnOptions::Axi(FnAxi::Write {
                addr,
                data: data.as_ptr(),
                len: data.len() as u32,
            }),
        )
    }

    fn noc_read(
        &self,
        noc_id: u8,
        x: u8,
        y: u8,
        addr: u64,
        data: &mut [u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        (self.callback)(
            &self.user_data,
            FnOptions::Noc(FnNoc::Read {
                noc_id,
                x: x as u32,
                y: y as u32,
                addr: addr as u64,
                data: data.as_mut_ptr(),
                len: data.len() as u64,
            }),
        )
    }

    fn noc_write(
        &self,
        noc_id: u8,
        x: u8,
        y: u8,
        addr: u64,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        (self.callback)(
            &self.user_data,
            FnOptions::Noc(FnNoc::Write {
                noc_id,
                x: x as u32,
                y: y as u32,
                addr,
                data: data.as_ptr(),
                len: data.len() as u64,
            }),
        )
    }

    fn noc_broadcast(
        &self,
        noc_id: u8,
        addr: u64,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        (self.callback)(
            &self.user_data,
            FnOptions::Noc(FnNoc::Broadcast {
                noc_id,
                addr: addr as u64,
                data: data.as_ptr(),
                len: data.len() as u64,
            }),
        )
    }

    fn eth_noc_read(
        &self,
        eth_addr: EthAddr,
        noc_id: u8,
        x: u8,
        y: u8,
        addr: u64,
        data: &mut [u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        (self.callback)(
            &self.user_data,
            FnOptions::Eth(FnRemote {
                addr: eth_addr,
                rw: FnNoc::Read {
                    noc_id,
                    x: x as u32,
                    y: y as u32,
                    addr: addr as u64,
                    data: data.as_mut_ptr(),
                    len: data.len() as u64,
                },
            }),
        )
    }

    fn eth_noc_write(
        &self,
        eth_addr: EthAddr,
        noc_id: u8,
        x: u8,
        y: u8,
        addr: u64,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        (self.callback)(
            &self.user_data,
            FnOptions::Eth(FnRemote {
                addr: eth_addr,
                rw: FnNoc::Write {
                    noc_id,
                    x: x as u32,
                    y: y as u32,
                    addr: addr as u64,
                    data: data.as_ptr(),
                    len: data.len() as u64,
                },
            }),
        )
    }

    fn eth_noc_broadcast(
        &self,
        eth_addr: EthAddr,
        noc_id: u8,
        addr: u64,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        (self.callback)(
            &self.user_data,
            FnOptions::Eth(FnRemote {
                addr: eth_addr,
                rw: FnNoc::Broadcast {
                    noc_id,
                    addr: addr as u64,
                    data: data.as_ptr(),
                    len: data.len() as u64,
                },
            }),
        )
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
