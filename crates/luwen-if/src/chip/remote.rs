use super::{eth_addr::EthAddr, HlComms, MemorySlices, Wormhole};
use crate::chip::communication::{
    chip_comms::{axi_translate, AxiData, AxiError, ChipComms},
    chip_interface::ChipInterface,
};

pub struct RemoteArcIf {
    pub addr: EthAddr,
    pub axi_data: Option<MemorySlices>,
}

impl ChipComms for RemoteArcIf {
    fn axi_translate(&self, addr: &str) -> Result<AxiData, AxiError> {
        axi_translate(self.axi_data.as_ref(), addr)
    }

    fn axi_read(&self, chip_if: &dyn ChipInterface, addr: u64, data: &mut [u8]) {
        chip_if.eth_noc_read(self.addr, 0, 0, 10, addr, data)
    }

    fn axi_write(&self, chip_if: &dyn ChipInterface, addr: u64, data: &[u8]) {
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
    ) {
        chip_if.eth_noc_read(self.addr, noc_id, x, y, addr, data);
    }

    fn noc_write(
        &self,
        chip_if: &dyn ChipInterface,
        noc_id: u8,
        x: u8,
        y: u8,
        addr: u64,
        data: &[u8],
    ) {
        chip_if.eth_noc_write(self.addr, noc_id, x, y, addr, data);
    }

    fn noc_broadcast(&self, chip_if: &dyn ChipInterface, noc_id: u8, addr: u64, data: &[u8]) {
        chip_if.eth_noc_broadcast(self.addr, noc_id, addr, data)
    }
}

#[derive(Clone)]
pub struct EthAddresses {
    pub boot_params: u64,
    pub node_info: u64,
    pub eth_conn_info: u64,
    pub debug_buf: u64,
    pub results_buf: u64,
    pub shelf_rack_routing: bool,
    pub heartbeat: u64,
    pub erisc_app: u64,
    pub erisc_app_config: u64,
}

impl EthAddresses {
    pub fn new(fw_version: u32) -> Self {
        let masked_version = fw_version & 0x00FFFFFF;

        let boot_params;
        let node_info;
        let eth_conn_info;
        let debug_buf;
        let results_buf;
        let shelf_rack_routing;
        let heartbeat;
        let erisc_app;
        let erisc_app_config;

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
            heartbeat = 0x1c;
            erisc_app = 0x9040;
            erisc_app_config = 0x12000;
        } else {
            heartbeat = 0x1f80;
            erisc_app = 0x8020;
            erisc_app_config = 0x12000;
        }

        EthAddresses {
            boot_params,
            node_info,
            eth_conn_info,
            debug_buf,
            results_buf,
            shelf_rack_routing,
            heartbeat,
            erisc_app,
            erisc_app_config,
        }
    }
}

impl Wormhole {
    pub fn get_local_chip_coord(&self) -> EthAddr {
        let coord = self.noc_read32(0, 9, 0, 0x1108);

        EthAddr {
            rack_x: (coord & 0xFF) as u8,
            rack_y: ((coord >> 8) & 0xFF) as u8,
            shelf_x: ((coord >> 16) & 0xFF) as u8,
            shelf_y: ((coord >> 24) & 0xFF) as u8,
        }
    }

    fn check_training_complete(&self) {
        let fw_version = self.noc_read32(0, 1, 0, 0x210);

        let eth_addrs = EthAddresses::new(fw_version);

        let initial_heartbeat = self
            .eth_locations
            .iter()
            .copied()
            .map(|(x, y)| self.noc_read32(0, x, y, self.eth_addres.heartbeat))
            .collect::<Vec<_>>();

        let start_time = std::time::Instant::now();

        loop {
            let heartbeat = self
                .eth_locations
                .iter()
                .copied()
                .map(|(x, y)| self.noc_read32(0, x, y, self.eth_addres.heartbeat));

            let valid_heartbeat = initial_heartbeat
                .iter()
                .copied()
                .zip(heartbeat)
                .map(|(h1, h2)| h1 != h2)
                .collect::<Vec<_>>();

            let init_finished = valid_heartbeat.iter().all(|&x| x);
            if init_finished {
                return;
            }

            let status_list = valid_heartbeat;

            let current_time = std::time::Instant::now();

            if current_time - start_time > std::time::Duration::from_secs(300) {
                panic!(
                    "Timed out after {:?} seconds while waiting for ethernet links to train.",
                    current_time - start_time
                );
            }
        }
    }
}
