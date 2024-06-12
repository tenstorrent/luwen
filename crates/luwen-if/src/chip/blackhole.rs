// SPDX-FileCopyrightText: © 2024 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{backtrace, sync::Arc};

use crate::{
    arc_msg::{ArcMsgAddr, ArcMsgOk, TypedArcMsg},
    chip::{
        communication::{
            chip_comms::{load_axi_table, ChipComms},
            chip_interface::ChipInterface,
        },
        hl_comms::HlCommsInterface,
    },
    error::{BtWrapper, PlatformError},
    ArcMsg, ChipImpl, IntoChip,
};

use super::{
    eth_addr::EthAddr,
    hl_comms::HlComms,
    init::status::{ComponentStatusInfo, EthernetPartialInitError, InitOptions, WaitStatus},
    remote::{EthAddresses, RemoteArcIf},
    ArcMsgOptions, ChipInitResult, CommsStatus, InitStatus, NeighbouringChip,
};

#[derive(Clone)]
pub struct Blackhole {
    pub chip_if: Arc<dyn ChipInterface + Send + Sync>,
    pub arc_if: Arc<dyn ChipComms + Send + Sync>,

    pub arc_addrs: ArcMsgAddr,
    pub eth_locations: [EthCore; 14],
    pub eth_addrs: EthAddresses,

    telemetry_addr: Arc<once_cell::sync::OnceCell<u32>>,
}

impl HlComms for Blackhole {
    fn comms_obj(&self) -> (&dyn ChipComms, &dyn ChipInterface) {
        (self.arc_if.as_ref(), self.chip_if.as_ref())
    }
}

impl HlComms for &Blackhole {
    fn comms_obj(&self) -> (&dyn ChipComms, &dyn ChipInterface) {
        (self.arc_if.as_ref(), self.chip_if.as_ref())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct L2Core {
    pub x: u8,
    pub y: u8,
}

#[derive(Clone, Copy, Debug)]
pub struct EthCore {
    pub x: u8,
    pub y: u8,
    pub enabled: bool,
}

impl Default for EthCore {
    fn default() -> Self {
        Self {
            x: 0,
            y: 0,
            enabled: true,
        }
    }
}

impl Blackhole {
    pub(crate) fn init<
        CC: ChipComms + Send + Sync + 'static,
        CI: ChipInterface + Send + Sync + 'static,
    >(
        arc_if: CC,
        chip_if: CI,
    ) -> Result<Self, PlatformError> {
        // let mut version = [0; 4];
        // arc_if.axi_read(&chip_if, 0x0, &mut version);
        // let version = u32::from_le_bytes(version);
        let _version = 0x0;

        let output = Blackhole {
            chip_if: Arc::new(chip_if),

            arc_addrs: ArcMsgAddr {
                scratch_base: arc_if.axi_translate("arc_ss.reset_unit.SCRATCH_0")?.addr,
                arc_misc_cntl: arc_if.axi_translate("arc_ss.reset_unit.ARC_MISC_CNTL")?.addr,
            },

            arc_if: Arc::new(arc_if),
            eth_addrs: EthAddresses::default(),

            telemetry_addr: Arc::new(once_cell::sync::OnceCell::new()),

            eth_locations: [
                EthCore {
                    x: 1,
                    y: 1,
                    ..Default::default()
                },
                EthCore {
                    x: 16,
                    y: 1,
                    ..Default::default()
                },
                EthCore {
                    x: 2,
                    y: 1,
                    ..Default::default()
                },
                EthCore {
                    x: 15,
                    y: 1,
                    ..Default::default()
                },
                EthCore {
                    x: 3,
                    y: 1,
                    ..Default::default()
                },
                EthCore {
                    x: 14,
                    y: 1,
                    ..Default::default()
                },
                EthCore {
                    x: 4,
                    y: 1,
                    ..Default::default()
                },
                EthCore {
                    x: 13,
                    y: 1,
                    ..Default::default()
                },
                EthCore {
                    x: 5,
                    y: 1,
                    ..Default::default()
                },
                EthCore {
                    x: 12,
                    y: 1,
                    ..Default::default()
                },
                EthCore {
                    x: 6,
                    y: 1,
                    ..Default::default()
                },
                EthCore {
                    x: 11,
                    y: 1,
                    ..Default::default()
                },
                EthCore {
                    x: 7,
                    y: 1,
                    ..Default::default()
                },
                EthCore {
                    x: 10,
                    y: 1,
                    ..Default::default()
                },
            ],
        };

        Ok(output)
    }

    pub fn init_eth_addrs(&mut self) -> Result<(), PlatformError> {
        if self.eth_addrs.masked_version == 0 {
            let telemetry = self.get_telemetry()?;

            self.eth_addrs = EthAddresses::new(telemetry.smbus_tx_eth_fw_version);
        }

        Ok(())
    }

    pub fn get_if<T: ChipInterface>(&self) -> Option<&T> {
        self.chip_if.as_any().downcast_ref::<T>()
    }

    pub fn spi_write(&self, addr: u32, value: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        unimplemented!("Need CMFW support for spi_write impl");

        Ok(())
    }

    pub fn spi_read(&self, addr: u32, value: &mut [u8]) -> Result<(), Box<dyn std::error::Error>> {
        unimplemented!("Need CMFW support for spi_read impl");

        Ok(())
    }
}

fn default_status() -> InitStatus {
    InitStatus {
        comms_status: super::CommsStatus::CanCommunicate,
        arc_status: ComponentStatusInfo {
            name: "ARC".to_string(),
            wait_status: Box::new([WaitStatus::Waiting(None)]),

            start_time: std::time::Instant::now(),
            timeout: std::time::Duration::from_secs(300),
        },
        dram_status: ComponentStatusInfo::init_waiting(
            "DRAM".to_string(),
            std::time::Duration::from_secs(300),
            8,
        ),
        eth_status: ComponentStatusInfo::init_waiting(
            "ETH".to_string(),
            std::time::Duration::from_secs(15 * 60),
            14,
        ),
        cpu_status: ComponentStatusInfo::init_waiting(
            "CPU".to_string(),
            std::time::Duration::from_secs(60),
            4,
        ),

        init_options: InitOptions { noc_safe: false },

        unknown_state: false,
    }
}

impl ChipImpl for Blackhole {
    // For now just assume the chip is fully working
    // will implement proper status checking later
    fn update_init_state(
        &mut self,
        status: &mut InitStatus,
    ) -> Result<ChipInitResult, PlatformError> {
        if status.unknown_state {
            let init_options = std::mem::take(&mut status.init_options);
            *status = default_status();
            status.init_options = init_options;
        }

        {
            let comms = &mut status.comms_status;
            *comms = match self.axi_sread32("arc_ss.reset_unit.SCRATCH_0") {
                Ok(_) => CommsStatus::CanCommunicate,
                Err(err) => CommsStatus::CommunicationError(err.to_string()),
            }
        }

        {
            let status = &mut status.arc_status;
            for s in status.wait_status.iter_mut() {
                *s = WaitStatus::Done;
            }
        }

        {
            let status = &mut status.dram_status;
            for s in status.wait_status.iter_mut() {
                *s = WaitStatus::Done;
            }
        }

        {
            let status = &mut status.eth_status;
            for s in status.wait_status.iter_mut() {
                *s = WaitStatus::Done;
            }
        }

        {
            let status = &mut status.cpu_status;
            for s in status.wait_status.iter_mut() {
                *s = WaitStatus::Done;
            }
        }

        Ok(ChipInitResult::NoError)
    }

    fn get_arch(&self) -> luwen_core::Arch {
        luwen_core::Arch::Blackhole
    }

    fn arc_msg(&self, msg: ArcMsgOptions) -> Result<ArcMsgOk, PlatformError> {
        unimplemented!("No arc_msg support")
    }

    fn get_neighbouring_chips(&self) -> Result<Vec<NeighbouringChip>, crate::error::PlatformError> {
        Ok(vec![])
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn get_telemetry(&self) -> Result<super::Telemetry, PlatformError> {
        unimplemented!("No telemetry support")
    }

    fn get_device_info(&self) -> Result<Option<crate::DeviceInfo>, PlatformError> {
        Ok(self.chip_if.get_device_info()?)
    }
}
