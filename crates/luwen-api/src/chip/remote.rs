// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{eth_addr::EthAddr, HlComms, MemorySlices, Wormhole};
use crate::{
    chip::communication::{
        chip_comms::{axi_translate, AxiData, AxiError, ChipComms},
        chip_interface::ChipInterface,
    },
    error::PlatformError,
};

pub struct RemoteArcIf {
    pub addr: EthAddr,
    pub axi_data: Option<MemorySlices>,
}

impl ChipComms for RemoteArcIf {
    fn axi_translate(&self, addr: &str) -> Result<AxiData, AxiError> {
        axi_translate(self.axi_data.as_ref(), addr)
    }

    fn axi_read(
        &self,
        chip_if: &dyn ChipInterface,
        addr: u64,
        data: &mut [u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        chip_if.eth_noc_read(self.addr, 0, 0, 10, addr, data)
    }

    fn axi_write(
        &self,
        chip_if: &dyn ChipInterface,
        addr: u64,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        chip_if.eth_noc_write(self.addr, 0, 0, 10, addr, data)
    }

    fn noc_read(
        &self,
        chip_if: &dyn ChipInterface,
        noc_id: u8,
        x: u8,
        y: u8,
        addr: u64,
        data: &mut [u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        chip_if.eth_noc_read(self.addr, noc_id, x, y, addr, data)
    }

    fn noc_write(
        &self,
        chip_if: &dyn ChipInterface,
        noc_id: u8,
        x: u8,
        y: u8,
        addr: u64,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        chip_if.eth_noc_write(self.addr, noc_id, x, y, addr, data)
    }

    fn noc_multicast(
        &self,
        chip_if: &dyn ChipInterface,
        noc_id: u8,
        start: (u8, u8),
        end: (u8, u8),
        addr: u64,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        chip_if.eth_noc_multicast(self.addr, noc_id, start, end, addr, data)
    }

    fn noc_broadcast(
        &self,
        chip_if: &dyn ChipInterface,
        noc_id: u8,
        addr: u64,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        chip_if.eth_noc_broadcast(self.addr, noc_id, addr, data)
    }
}

#[derive(Clone, Default)]
pub struct EthAddresses {
    pub masked_version: u32,

    pub version: u64,
    pub boot_params: u64,
    pub node_info: u64,
    pub eth_conn_info: u64,
    pub debug_buf: u64,
    pub results_buf: u64,
    pub shelf_rack_routing: bool,
    pub heartbeat: u64,
    pub erisc_app: u64,
    pub erisc_app_config: u64,
    pub erisc_remote_board_type_offset: u64,
    pub erisc_local_board_type_offset: u64,
}

impl EthAddresses {
    pub fn new(fw_version: u32) -> Self {
        let masked_version = fw_version & 0x00FFFFFF;

        let version;
        let boot_params;
        let node_info;
        let eth_conn_info;
        let debug_buf;
        let results_buf;
        let shelf_rack_routing;
        let heartbeat;
        let erisc_app;
        let erisc_app_config;
        let erisc_remote_board_type_offset;
        let erisc_local_board_type_offset;

        if masked_version >= 0x050000 {
            boot_params = 0x1000;
            node_info = 0x1100;
            eth_conn_info = 0x1200;
            debug_buf = 0x12c0;
            results_buf = 0x1ec0;
            shelf_rack_routing = true;
        } else if masked_version >= 0x030000 {
            boot_params = 0x1000;
            node_info = 0x1100;
            eth_conn_info = 0x1200;
            debug_buf = 0x1240;
            results_buf = 0x1e40;
            shelf_rack_routing = false;
        } else {
            boot_params = 0x5000;
            node_info = 0x5100;
            eth_conn_info = 0x5200;
            debug_buf = 0x5240;
            results_buf = 0x5e40;
            shelf_rack_routing = false;
        }

        if masked_version >= 0x060000 {
            version = 0x210;
            heartbeat = 0x1c;
            erisc_app = 0x9040;
            erisc_app_config = 0x12000;
        } else {
            version = 0x210;
            heartbeat = 0x1f80;
            erisc_app = 0x8020;
            erisc_app_config = 0x12000;
        }

        if masked_version >= 0x06C000 {
            erisc_remote_board_type_offset = 77;
            erisc_local_board_type_offset = 69;
        } else {
            erisc_remote_board_type_offset = 72;
            erisc_local_board_type_offset = 64;
        }

        EthAddresses {
            version,
            masked_version,
            boot_params,
            node_info,
            eth_conn_info,
            debug_buf,
            results_buf,
            shelf_rack_routing,
            heartbeat,
            erisc_app,
            erisc_app_config,
            erisc_remote_board_type_offset,
            erisc_local_board_type_offset,
        }
    }
}

impl Wormhole {
    pub fn get_local_chip_coord(&self) -> Result<EthAddr, PlatformError> {
        let coord = self.noc_read32(0, 9, 0, self.eth_addrs.node_info + 8)?;

        Ok(EthAddr {
            rack_x: (coord & 0xFF) as u8,
            rack_y: ((coord >> 8) & 0xFF) as u8,
            shelf_x: ((coord >> 16) & 0xFF) as u8,
            shelf_y: ((coord >> 24) & 0xFF) as u8,
        })
    }

    pub(crate) fn check_ethernet_training_complete(&mut self) -> Result<Vec<bool>, PlatformError> {
        self.init_eth_addrs()?;

        let mut initial_heartbeat = Vec::with_capacity(self.eth_locations.len());
        for core in self.eth_locations.iter() {
            if core.enabled {
                initial_heartbeat.push(Some(self.noc_read32(
                    0,
                    core.x,
                    core.y,
                    self.eth_addrs.heartbeat,
                )?));
            } else {
                initial_heartbeat.push(None);
            }
        }

        let start_time = std::time::Instant::now();

        // During initial training the erisc cores aren't running their heartbeats. In addition
        // ethernet needs active retraining after initial training has completed. This retraining
        // can only occur if the heartbeat is running. Therefore if the heartbeat is not running I
        // assume that the link is not retrained.
        //
        // This procedure will block for 100 ms because I did not want to add state to the Wormhole
        // struct to track the last time a heartbeat was incremented on each core because this
        // function is only called during initialization.
        let mut heartbeat = Vec::with_capacity(self.eth_locations.len());
        loop {
            heartbeat.clear();
            for core in self.eth_locations.iter() {
                if core.enabled {
                    heartbeat.push(Some(self.noc_read32(
                        0,
                        core.x,
                        core.y,
                        self.eth_addrs.heartbeat,
                    )?));
                } else {
                    heartbeat.push(None);
                }
            }

            let valid_heartbeat = initial_heartbeat
                .iter_mut()
                .zip(heartbeat.iter().copied())
                .map(|(h1, h2)| {
                    if h1.is_none() && h2.is_some() {
                        *h1 = h2
                    }
                    *h1 != h2
                })
                .collect::<Vec<_>>();

            let init_finished = valid_heartbeat.iter().all(|&x| x);
            if init_finished || start_time.elapsed() > std::time::Duration::from_millis(100) {
                return Ok(valid_heartbeat);
            }
        }
    }

    pub(crate) fn check_ethernet_fw_version(&mut self) -> Result<Vec<bool>, PlatformError> {
        let mut valid_fw_version = Vec::with_capacity(self.eth_locations.len());
        for core in &self.eth_locations {
            let eth_fw_version = self.eth_addrs.masked_version;
            let msbyte = (eth_fw_version >> 24) & 0xFF;
            if msbyte != 0x0
                || msbyte != 0x6
                || self.noc_read32(0, core.x, core.y, self.eth_addrs.version)? & 0x00FFFFFF
                    != eth_fw_version
            {
                valid_fw_version.push(true);
            } else {
                valid_fw_version.push(false);
            }
        }

        Ok(valid_fw_version)
    }
}
