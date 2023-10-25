// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::{Deref, DerefMut};

use luwen_core::Arch;
use luwen_if::chip::{
    wait_for_init, ArcMsg, ArcMsgOk, ArcMsgOptions, ChipImpl, HlComms, HlCommsInterface,
};
use luwen_if::{CallbackStorage, ChipDetectOptions, DeviceInfo, UninitChip};
use luwen_ref::{DmaConfig, ExtendedPciDeviceWrapper};
use pyo3::exceptions::PyException;
use pyo3::prelude::*;

#[pyclass]
pub struct PciChip(luwen_if::chip::Chip);

impl Deref for PciChip {
    type Target = luwen_if::chip::Chip;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for PciChip {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[pyclass]
pub struct PciWormhole(luwen_if::chip::Wormhole);

impl Deref for PciWormhole {
    type Target = luwen_if::chip::Wormhole;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for PciWormhole {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[pyclass]
pub struct PciGrayskull(luwen_if::chip::Grayskull);

impl Deref for PciGrayskull {
    type Target = luwen_if::chip::Grayskull;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for PciGrayskull {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[pyclass]
pub struct DmaBuffer(luwen_ref::DmaBuffer);

#[pymethods]
impl DmaBuffer {
    pub fn get_user_address(&self) -> u64 {
        self.0.buffer.as_ptr() as u64
    }

    pub fn get_physical_address(&self) -> u64 {
        self.0.physical_address
    }
}

#[pyclass]
pub struct Telemetry {
    #[pyo3(get)]
    board_id: u64,
    #[pyo3(get)]
    smbus_tx_enum_version: u32,
    #[pyo3(get)]
    smbus_tx_device_id: u32,
    #[pyo3(get)]
    smbus_tx_asic_ro: u32,
    #[pyo3(get)]
    smbus_tx_asic_idd: u32,
    #[pyo3(get)]
    smbus_tx_board_id_high: u32,
    #[pyo3(get)]
    smbus_tx_board_id_low: u32,
    #[pyo3(get)]
    smbus_tx_arc0_fw_version: u32,
    #[pyo3(get)]
    smbus_tx_arc1_fw_version: u32,
    #[pyo3(get)]
    smbus_tx_arc2_fw_version: u32,
    #[pyo3(get)]
    smbus_tx_arc3_fw_version: u32,
    #[pyo3(get)]
    smbus_tx_spibootrom_fw_version: u32,
    #[pyo3(get)]
    smbus_tx_eth_fw_version: u32,
    #[pyo3(get)]
    smbus_tx_m3_bl_fw_version: u32,
    #[pyo3(get)]
    smbus_tx_m3_app_fw_version: u32,
    #[pyo3(get)]
    smbus_tx_ddr_speed: Option<u32>,
    #[pyo3(get)]
    smbus_tx_ddr_status: u32,
    #[pyo3(get)]
    smbus_tx_eth_status0: u32,
    #[pyo3(get)]
    smbus_tx_eth_status1: u32,
    #[pyo3(get)]
    smbus_tx_pcie_status: u32,
    #[pyo3(get)]
    smbus_tx_faults: u32,
    #[pyo3(get)]
    smbus_tx_arc0_health: u32,
    #[pyo3(get)]
    smbus_tx_arc1_health: u32,
    #[pyo3(get)]
    smbus_tx_arc2_health: u32,
    #[pyo3(get)]
    smbus_tx_arc3_health: u32,
    #[pyo3(get)]
    smbus_tx_fan_speed: u32,
    #[pyo3(get)]
    smbus_tx_aiclk: u32,
    #[pyo3(get)]
    smbus_tx_axiclk: u32,
    #[pyo3(get)]
    smbus_tx_arcclk: u32,
    #[pyo3(get)]
    smbus_tx_throttler: u32,
    #[pyo3(get)]
    smbus_tx_vcore: u32,
    #[pyo3(get)]
    smbus_tx_asic_temperature: u32,
    #[pyo3(get)]
    smbus_tx_vreg_temperature: u32,
    #[pyo3(get)]
    smbus_tx_board_temperature: u32,
    #[pyo3(get)]
    smbus_tx_tdp: u32,
    #[pyo3(get)]
    smbus_tx_tdc: u32,
    #[pyo3(get)]
    smbus_tx_vdd_limits: u32,
    #[pyo3(get)]
    smbus_tx_thm_limits: u32,
    #[pyo3(get)]
    smbus_tx_wh_fw_date: u32,
    #[pyo3(get)]
    smbus_tx_asic_tmon0: u32,
    #[pyo3(get)]
    smbus_tx_asic_tmon1: u32,
    #[pyo3(get)]
    smbus_tx_mvddq_power: u32,
    #[pyo3(get)]
    smbus_tx_gddr_train_temp0: u32,
    #[pyo3(get)]
    smbus_tx_gddr_train_temp1: u32,
    #[pyo3(get)]
    smbus_tx_boot_date: u32,
    #[pyo3(get)]
    smbus_tx_rt_seconds: u32,
    #[pyo3(get)]
    smbus_tx_eth_debug_status0: u32,
    #[pyo3(get)]
    smbus_tx_eth_debug_status1: u32,
    #[pyo3(get)]
    smbus_tx_tt_flash_version: u32,
}
impl From<luwen_if::chip::Telemetry> for Telemetry {
    fn from(value: luwen_if::chip::Telemetry) -> Self {
        Self {
            board_id: value.board_id,
            smbus_tx_enum_version: value.smbus_tx_enum_version,
            smbus_tx_device_id: value.smbus_tx_device_id,
            smbus_tx_asic_ro: value.smbus_tx_asic_ro,
            smbus_tx_asic_idd: value.smbus_tx_asic_idd,
            smbus_tx_board_id_high: value.smbus_tx_board_id_high,
            smbus_tx_board_id_low: value.smbus_tx_board_id_low,
            smbus_tx_arc0_fw_version: value.smbus_tx_arc0_fw_version,
            smbus_tx_arc1_fw_version: value.smbus_tx_arc1_fw_version,
            smbus_tx_arc2_fw_version: value.smbus_tx_arc2_fw_version,
            smbus_tx_arc3_fw_version: value.smbus_tx_arc3_fw_version,
            smbus_tx_spibootrom_fw_version: value.smbus_tx_spibootrom_fw_version,
            smbus_tx_eth_fw_version: value.smbus_tx_eth_fw_version,
            smbus_tx_m3_bl_fw_version: value.smbus_tx_m3_bl_fw_version,
            smbus_tx_m3_app_fw_version: value.smbus_tx_m3_app_fw_version,
            smbus_tx_ddr_speed: value.smbus_tx_ddr_speed,
            smbus_tx_ddr_status: value.smbus_tx_ddr_status,
            smbus_tx_eth_status0: value.smbus_tx_eth_status0,
            smbus_tx_eth_status1: value.smbus_tx_eth_status1,
            smbus_tx_pcie_status: value.smbus_tx_pcie_status,
            smbus_tx_faults: value.smbus_tx_faults,
            smbus_tx_arc0_health: value.smbus_tx_arc0_health,
            smbus_tx_arc1_health: value.smbus_tx_arc1_health,
            smbus_tx_arc2_health: value.smbus_tx_arc2_health,
            smbus_tx_arc3_health: value.smbus_tx_arc3_health,
            smbus_tx_fan_speed: value.smbus_tx_fan_speed,
            smbus_tx_aiclk: value.smbus_tx_aiclk,
            smbus_tx_axiclk: value.smbus_tx_axiclk,
            smbus_tx_arcclk: value.smbus_tx_arcclk,
            smbus_tx_throttler: value.smbus_tx_throttler,
            smbus_tx_vcore: value.smbus_tx_vcore,
            smbus_tx_asic_temperature: value.smbus_tx_asic_temperature,
            smbus_tx_vreg_temperature: value.smbus_tx_vreg_temperature,
            smbus_tx_board_temperature: value.smbus_tx_board_temperature,
            smbus_tx_tdp: value.smbus_tx_tdp,
            smbus_tx_tdc: value.smbus_tx_tdc,
            smbus_tx_vdd_limits: value.smbus_tx_vdd_limits,
            smbus_tx_thm_limits: value.smbus_tx_thm_limits,
            smbus_tx_wh_fw_date: value.smbus_tx_wh_fw_date,
            smbus_tx_asic_tmon0: value.smbus_tx_asic_tmon0,
            smbus_tx_asic_tmon1: value.smbus_tx_asic_tmon1,
            smbus_tx_mvddq_power: value.smbus_tx_mvddq_power,
            smbus_tx_gddr_train_temp0: value.smbus_tx_gddr_train_temp0,
            smbus_tx_gddr_train_temp1: value.smbus_tx_gddr_train_temp1,
            smbus_tx_boot_date: value.smbus_tx_boot_date,
            smbus_tx_rt_seconds: value.smbus_tx_rt_seconds,
            smbus_tx_eth_debug_status0: value.smbus_tx_eth_debug_status0,
            smbus_tx_eth_debug_status1: value.smbus_tx_eth_debug_status1,
            smbus_tx_tt_flash_version: value.smbus_tx_tt_flash_version,
        }
    }
}

#[pyclass]
pub struct AxiData {
    #[pyo3(get)]
    addr: u64,
    #[pyo3(get)]
    size: u64,
}

impl From<luwen_if::chip::AxiData> for AxiData {
    fn from(value: luwen_if::chip::AxiData) -> Self {
        Self {
            addr: value.addr,
            size: value.size,
        }
    }
}

macro_rules! common_chip_comms_impls {
    ($name:ty) => {
        #[pymethods]
        impl $name {
            pub fn noc_read(
                &self,
                noc_id: u8,
                x: u8,
                y: u8,
                addr: u64,
                data: pyo3::buffer::PyBuffer<u8>,
            ) -> PyResult<()> {
                Python::with_gil(|_py| {
                    let ptr: *mut u8 = data.buf_ptr().cast();
                    let len = data.len_bytes();

                    let data = unsafe { std::slice::from_raw_parts_mut(ptr, len) };
                    self.0
                        .noc_read(noc_id, x, y, addr, data)
                        .map_err(|v| PyException::new_err(v.to_string()))
                })
            }

            pub fn noc_read32(&self, noc_id: u8, x: u8, y: u8, addr: u64) -> PyResult<u32> {
                let mut data = [0u8; 4];
                self.0
                    .noc_read(noc_id, x, y, addr, &mut data)
                    .map_err(|v| PyException::new_err(v.to_string()))?;

                Ok(u32::from_le_bytes(data))
            }

            pub fn noc_write(
                &self,
                noc_id: u8,
                x: u8,
                y: u8,
                addr: u64,
                data: pyo3::buffer::PyBuffer<u8>,
            ) -> PyResult<()> {
                Python::with_gil(|_py| {
                    let ptr: *mut u8 = data.buf_ptr().cast();
                    let len = data.len_bytes();

                    let data = unsafe { std::slice::from_raw_parts(ptr, len) };
                    self.0
                        .noc_write(noc_id, x, y, addr, data)
                        .map_err(|v| PyException::new_err(v.to_string()))
                })
            }

            pub fn noc_write32(
                &self,
                noc_id: u8,
                x: u8,
                y: u8,
                addr: u64,
                data: u32,
            ) -> PyResult<()> {
                self.0
                    .noc_write(noc_id, x, y, addr, &data.to_le_bytes())
                    .map_err(|v| PyException::new_err(v.to_string()))
            }

            pub fn noc_broadcast(
                &self,
                noc_id: u8,
                addr: u64,
                data: pyo3::buffer::PyBuffer<u8>,
            ) -> PyResult<()> {
                Python::with_gil(|_py| {
                    let ptr: *mut u8 = data.buf_ptr().cast();
                    let len = data.len_bytes();

                    let data = unsafe { std::slice::from_raw_parts(ptr, len) };
                    self.0
                        .noc_broadcast(noc_id, addr, data)
                        .map_err(|v| PyException::new_err(v.to_string()))
                })
            }

            pub fn noc_broadcast32(&self, noc_id: u8, addr: u64, data: u32) -> PyResult<()> {
                self.0
                    .noc_broadcast(noc_id, addr, &data.to_le_bytes())
                    .map_err(|v| PyException::new_err(v.to_string()))
            }

            pub fn axi_translate(&self, addr: &str) -> PyResult<AxiData> {
                match self.0.axi_translate(addr).map_err(|err| err.to_string()) {
                    Ok(v) => Ok(v.into()),
                    Err(err) => Err(PyException::new_err(err)),
                }
            }

            pub fn axi_read(&self, addr: u64, data: pyo3::buffer::PyBuffer<u8>) -> PyResult<()> {
                Python::with_gil(|_py| {
                    let ptr: *mut u8 = data.buf_ptr().cast();
                    let len = data.len_bytes();

                    let data = unsafe { std::slice::from_raw_parts_mut(ptr, len) };
                    self.0
                        .axi_read(addr, data)
                        .map_err(|v| PyException::new_err(v.to_string()))
                })
            }

            pub fn axi_read32(&self, addr: u64) -> PyResult<u32> {
                let mut data = [0u8; 4];
                self.0
                    .axi_read(addr, &mut data)
                    .map_err(|v| PyException::new_err(v.to_string()))?;

                Ok(u32::from_le_bytes(data))
            }

            pub fn axi_write(&self, addr: u64, data: pyo3::buffer::PyBuffer<u8>) -> PyResult<()> {
                Python::with_gil(|_py| {
                    let ptr: *mut u8 = data.buf_ptr().cast();
                    let len = data.len_bytes();

                    let data = unsafe { std::slice::from_raw_parts_mut(ptr, len) };
                    self.0
                        .axi_write(addr, data)
                        .map_err(|v| PyException::new_err(v.to_string()))
                })
            }

            pub fn axi_write32(&self, addr: u64, data: u32) -> PyResult<()> {
                self.0
                    .axi_write(addr, &data.to_le_bytes())
                    .map_err(|v| PyException::new_err(v.to_string()))
            }

            #[pyo3(signature = (msg, wait_for_done = true, use_second_mailbox = false, arg0 = 0xffff, arg1 = 0xffff, timeout = 1.0))]
            pub fn arc_msg(&self, msg: u32, wait_for_done: bool, use_second_mailbox: bool, arg0: u16, arg1: u16, timeout: f64) -> PyResult<Option<(u32, u32)>> {
                match self.0
                    .arc_msg(ArcMsgOptions {
                        addrs: None,
                        msg: ArcMsg::from_values(msg, arg0, arg1),
                        wait_for_done,
                        use_second_mailbox,
                        timeout: std::time::Duration::from_secs_f64(timeout),
                    }) {
                        Ok(ArcMsgOk::Ok {rc, arg}) => {
                            Ok(Some((arg, rc)))
                        }
                        Ok(ArcMsgOk::OkNoWait) => {
                            Ok(None)
                        }
                        Err(err) => {
                            Err(PyException::new_err(err.to_string()))
                        }
                    }
            }
        }
    };
}

impl PciChip {
    fn device_info(&self) -> PyResult<DeviceInfo> {
        match self.0.inner.get_device_info() {
            Ok(info) => {
                if let Some(info) = info {
                    Ok(info)
                } else {
                    return Err(PyException::new_err(
                        "Could not get device info: info unavailable",
                    ));
                }
            }
            Err(err) => {
                return Err(PyException::new_err(format!(
                    "Could not get device info: {}",
                    err
                )));
            }
        }
    }
}

#[pymethods]
impl PciChip {
    pub fn as_wh(&self) -> Option<PciWormhole> {
        self.0.as_wh().map(|v| PciWormhole(v.clone()))
    }

    pub fn as_gs(&self) -> Option<PciGrayskull> {
        self.0.as_gs().map(|v| PciGrayskull(v.clone()))
    }

    pub fn is_remote(&self) -> bool {
        if let Some(wh) = self.0.as_wh() {
            wh.is_remote
        } else {
            false
        }
    }

    #[new]
    pub fn new(pci_interface: Option<usize>) -> Self {
        let pci_interface = pci_interface.unwrap();

        let chip = luwen_ref::ExtendedPciDevice::open(pci_interface).unwrap();

        let arch = chip.borrow().device.arch;

        PciChip(luwen_if::chip::Chip::open(
            arch,
            luwen_if::CallbackStorage {
                callback: luwen_ref::comms_callback,
                user_data: chip,
            },
        ))
    }

    pub fn init(&self) -> PyResult<()> {
        wait_for_init(&self.0, &mut |_| {}, false, false).map_err(|v| {
            PyException::new_err(format!("Could not initialize chip: {}", v.to_string()))
        })?;

        Ok(())
    }

    pub fn board_id(&self) -> u64 {
        self.0.inner.get_telemetry().unwrap().board_id
    }

    pub fn get_telemetry(&self) -> Telemetry {
        self.0.inner.get_telemetry().unwrap().into()
    }

    pub fn device_id(&self) -> PyResult<u32> {
        let info = self.device_info()?;
        Ok(((info.vendor as u32) << 16) | info.device_id as u32)
    }

    pub fn bar_size(&self) -> PyResult<u64> {
        let info = self.device_info()?;
        Ok(info.bar_size)
    }

    pub fn get_pci_bdf(&self) -> PyResult<String> {
        let info = self.device_info()?;
        Ok(format!(
            "{:04x}:{:02x}:{:02x}.{:x}",
            info.domain, info.bus, info.slot, info.function
        ))
    }

    pub fn get_pci_interface_id(&self) -> PyResult<u32> {
        let info = self.device_info()?;
        Ok(info.interface_id)
    }
}

common_chip_comms_impls!(PciChip);

#[pymethods]
impl PciGrayskull {
    pub fn setup_tlb(
        &mut self,
        index: u32,
        addr: u64,
        x_start: u8,
        y_start: u8,
        x_end: u8,
        y_end: u8,
        noc_sel: u8,
        mcast: bool,
        ordering: u8,
        linked: bool,
    ) -> PyResult<(u64, u64)> {
        let value = PciInterface::from_gs(self);

        if let Some(value) = value {
            match kmdif::tlb::Ordering::from(ordering) {
                kmdif::tlb::Ordering::UNKNOWN(ordering) => Err(PyException::new_err(format!(
                    "Invalid ordering {ordering}."
                ))),
                ordering => Ok(value.setup_tlb(
                    index, addr, x_start, y_start, x_end, y_end, noc_sel, mcast, ordering, linked,
                )),
            }
        } else {
            return Err(PyException::new_err(
                "Could not get PCI interface for this chip.",
            ));
        }
    }

    pub fn set_default_tlb(&self, index: u32) -> PyResult<()> {
        let value = PciInterface::from_gs(self);

        if let Some(value) = value {
            value.pci_interface.borrow_mut().default_tlb = index;
            Ok(())
        } else {
            return Err(PyException::new_err(
                "Could not get PCI interface for this chip.",
            ));
        }
    }

    pub fn pci_axi_read32(&self, addr: u32) -> PyResult<u32> {
        let value = PciInterface::from_gs(self);
        if let Some(value) = value {
            value
                .axi_read32(addr)
                .map_err(|v| PyException::new_err(v.to_string()))
        } else {
            return Err(PyException::new_err(
                "Could not get PCI interface for this chip.",
            ));
        }
    }

    pub fn pci_axi_write32(&self, addr: u32, data: u32) -> PyResult<()> {
        let value = PciInterface::from_gs(self);
        if let Some(value) = value {
            value
                .axi_write32(addr, data)
                .map_err(|v| PyException::new_err(v.to_string()))
        } else {
            return Err(PyException::new_err(
                "Could not get PCI interface for this chip.",
            ));
        }
    }

    pub fn pci_board_type(&self) -> PyResult<u16> {
        let value = PciInterface::from_gs(self);
        if let Some(value) = value {
            Ok(value.pci_interface.borrow().device.physical.subsystem_id)
        } else {
            return Err(PyException::new_err(
                "Could not get PCI interface for this chip.",
            ));
        }
    }

    pub fn spi_read(&self, addr: u32, data: pyo3::buffer::PyBuffer<u8>) -> PyResult<()> {
        Python::with_gil(|_py| {
            let ptr: *mut u8 = data.buf_ptr().cast();
            let len = data.len_bytes();

            let data = unsafe { std::slice::from_raw_parts_mut(ptr, len) };
            self.0
                .spi_read(addr, data)
                .map_err(|v| PyException::new_err(v.to_string()))
        })
    }

    pub fn spi_write(&self, addr: u32, data: pyo3::buffer::PyBuffer<u8>) -> PyResult<()> {
        Python::with_gil(|_py| {
            let ptr: *mut u8 = data.buf_ptr().cast();
            let len = data.len_bytes();

            let data = unsafe { std::slice::from_raw_parts(ptr, len) };
            self.0
                .spi_write(addr, data)
                .map_err(|v| PyException::new_err(v.to_string()))
        })
    }
}

common_chip_comms_impls!(PciGrayskull);

pub struct PciInterface<'a> {
    pub pci_interface: &'a ExtendedPciDeviceWrapper,
}

impl PciInterface<'_> {
    pub fn from_wh<'a>(wh: &'a PciWormhole) -> Option<PciInterface<'a>> {
        wh.0.get_if::<CallbackStorage<ExtendedPciDeviceWrapper>>()
            .map(|v| PciInterface {
                pci_interface: &v.user_data,
            })
    }

    pub fn from_gs<'a>(gs: &'a PciGrayskull) -> Option<PciInterface<'a>> {
        gs.0.get_if::<CallbackStorage<ExtendedPciDeviceWrapper>>()
            .map(|v| PciInterface {
                pci_interface: &v.user_data,
            })
    }

    pub fn setup_tlb(
        &self,
        index: u32,
        addr: u64,
        x_start: u8,
        y_start: u8,
        x_end: u8,
        y_end: u8,
        noc_sel: u8,
        mcast: bool,
        ordering: kmdif::tlb::Ordering,
        linked: bool,
    ) -> (u64, u64) {
        self.pci_interface
            .borrow_mut()
            .setup_tlb(
                index,
                kmdif::Tlb {
                    local_offset: addr,
                    x_end,
                    y_end,
                    x_start,
                    y_start,
                    noc_sel,
                    mcast,
                    ordering,
                    linked,
                },
            )
            .unwrap()
    }

    pub fn noc_read(&self, tlb_index: u32, addr: u64, data: &mut [u8]) {
        self.pci_interface
            .borrow_mut()
            .noc_read(tlb_index, addr, data)
            .unwrap();
    }

    pub fn noc_write(&self, tlb_index: u32, addr: u64, data: &[u8]) {
        self.pci_interface
            .borrow_mut()
            .noc_write(tlb_index, addr, data)
            .unwrap();
    }

    pub fn allocate_dma_buffer(&self, size: u32) -> Result<DmaBuffer, String> {
        let buffer = self
            .pci_interface
            .borrow_mut()
            .device
            .allocate_dma_buffer(size)
            .map_err(|v| v.to_string())?;
        Ok(DmaBuffer(buffer))
    }

    pub fn config_dma(
        &self,
        dma_64_bit_addr: Option<u32>,
        csm_pcie_ctrl_dma_request_offset: u32,
        arc_misc_cntl_addr: u32,
        msi: bool,
        read_threshold: u32,
        write_threshold: u32,
    ) -> Result<(), String> {
        let borrow: &mut _ = &mut self.pci_interface.borrow_mut();
        borrow.device.dma_config = Some(DmaConfig {
            csm_pcie_ctrl_dma_request_offset,
            arc_misc_cntl_addr,
            dma_host_phys_addr_high: dma_64_bit_addr.unwrap_or(0),
            support_64_bit_dma: dma_64_bit_addr.is_some(),
            use_msi_for_dma: msi,
            read_threshold,
            write_threshold,
        });

        Ok(())
    }

    pub fn dma_transfer_turbo(
        &self,
        addr: u32,
        physical_address: u64,
        size: u32,
        write: bool,
    ) -> Result<(), String> {
        let borrow: &mut _ = &mut self.pci_interface.borrow_mut();
        borrow
            .device
            .pcie_dma_transfer_turbo(addr, physical_address, size, write)
            .map_err(|v| v.to_string())
    }

    pub fn axi_write32(&self, addr: u32, value: u32) -> Result<(), String> {
        let borrow: &mut _ = &mut self.pci_interface.borrow_mut();
        borrow
            .device
            .write32(addr, value)
            .map_err(|v| v.to_string())
    }

    pub fn axi_read32(&self, addr: u32) -> Result<u32, String> {
        let borrow: &mut _ = &mut self.pci_interface.borrow_mut();
        borrow.device.read32(addr).map_err(|v| v.to_string())
    }
}

#[pymethods]
impl PciWormhole {
    pub fn open_remote(
        &self,
        rack_x: Option<u8>,
        rack_y: Option<u8>,
        shelf_x: Option<u8>,
        shelf_y: Option<u8>,
    ) -> PyResult<RemoteWormhole> {
        Ok(RemoteWormhole(
            self.0
                .open_remote((rack_x, rack_y, shelf_x, shelf_y))
                .map_err(|v| PyException::new_err(format!("Could not open remote: {}", v)))?,
        ))
    }

    pub fn setup_tlb(
        &mut self,
        index: u32,
        addr: u64,
        x_start: u8,
        y_start: u8,
        x_end: u8,
        y_end: u8,
        noc_sel: u8,
        mcast: bool,
        ordering: u8,
        linked: bool,
    ) -> PyResult<(u64, u64)> {
        let value = PciInterface::from_wh(self);

        if let Some(value) = value {
            match kmdif::tlb::Ordering::from(ordering) {
                kmdif::tlb::Ordering::UNKNOWN(ordering) => Err(PyException::new_err(format!(
                    "Invalid ordering {ordering}."
                ))),
                ordering => Ok(value.setup_tlb(
                    index, addr, x_start, y_start, x_end, y_end, noc_sel, mcast, ordering, linked,
                )),
            }
        } else {
            return Err(PyException::new_err(
                "Could not get PCI interface for this chip.",
            ));
        }
    }

    pub fn set_default_tlb(&self, index: u32) -> PyResult<()> {
        let value = PciInterface::from_wh(self);

        if let Some(value) = value {
            value.pci_interface.borrow_mut().default_tlb = index;
            Ok(())
        } else {
            return Err(PyException::new_err(
                "Could not get PCI interface for this chip.",
            ));
        }
    }

    pub fn allocate_dma_buffer(&self, size: u32) -> PyResult<DmaBuffer> {
        let value = PciInterface::from_wh(self);

        if let Some(value) = value {
            Ok(value.allocate_dma_buffer(size).map_err(|v| {
                PyException::new_err(format!("Could not allocate DMA buffer: {}", v))
            })?)
        } else {
            return Err(PyException::new_err(
                "Could not get PCI interface for this chip.",
            ));
        }
    }

    #[pyo3(signature = (dma_64_bit_addr, csm_pcie_ctrl_dma_request_offset, arc_misc_cntl_addr, msi, read_threshold, write_threshold))]
    pub fn config_dma(
        &self,
        dma_64_bit_addr: Option<u32>,
        csm_pcie_ctrl_dma_request_offset: u32,
        arc_misc_cntl_addr: u32,
        msi: bool,
        read_threshold: u32,
        write_threshold: u32,
    ) -> PyResult<()> {
        let value = PciInterface::from_wh(self);

        if let Some(value) = value {
            Ok(value
                .config_dma(
                    dma_64_bit_addr,
                    csm_pcie_ctrl_dma_request_offset,
                    arc_misc_cntl_addr,
                    msi,
                    read_threshold,
                    write_threshold,
                )
                .map_err(|v| PyException::new_err(format!("Could perform dma config: {}", v)))?)
        } else {
            return Err(PyException::new_err(
                "Could not get PCI interface for this chip.",
            ));
        }
    }

    pub fn dma_transfer_turbo(
        &self,
        addr: u32,
        physical_dma_buffer: u64,
        size: u32,
        write: bool,
    ) -> PyResult<()> {
        let value = PciInterface::from_wh(self);

        if let Some(value) = value {
            Ok(value
                .dma_transfer_turbo(addr, physical_dma_buffer, size, write)
                .map_err(|v| PyException::new_err(format!("Could perform dma transfer: {}", v)))?)
        } else {
            return Err(PyException::new_err(
                "Could not get PCI interface for this chip.",
            ));
        }
    }

    pub fn spi_read(&self, addr: u32, data: pyo3::buffer::PyBuffer<u8>) -> PyResult<()> {
        Python::with_gil(|_py| {
            let ptr: *mut u8 = data.buf_ptr().cast();
            let len = data.len_bytes();

            let data = unsafe { std::slice::from_raw_parts_mut(ptr, len) };
            self.0
                .spi_read(addr, data)
                .map_err(|v| PyException::new_err(v.to_string()))
        })
    }

    pub fn spi_write(&self, addr: u32, data: pyo3::buffer::PyBuffer<u8>) -> PyResult<()> {
        Python::with_gil(|_py| {
            let ptr: *mut u8 = data.buf_ptr().cast();
            let len = data.len_bytes();

            let data = unsafe { std::slice::from_raw_parts(ptr, len) };
            self.0
                .spi_write(addr, data)
                .map_err(|v| PyException::new_err(v.to_string()))
        })
    }
}

common_chip_comms_impls!(PciWormhole);

#[pyclass]
pub struct RemoteWormhole(luwen_if::chip::Wormhole);

common_chip_comms_impls!(RemoteWormhole);

impl RemoteWormhole {
    pub fn spi_read(&self, addr: u32, data: pyo3::buffer::PyBuffer<u8>) -> PyResult<()> {
        Python::with_gil(|_py| {
            let ptr: *mut u8 = data.buf_ptr().cast();
            let len = data.len_bytes();

            let data = unsafe { std::slice::from_raw_parts_mut(ptr, len) };
            self.0
                .spi_read(addr, data)
                .map_err(|v| PyException::new_err(v.to_string()))
        })
    }

    pub fn spi_write(&self, addr: u32, data: pyo3::buffer::PyBuffer<u8>) -> PyResult<()> {
        Python::with_gil(|_py| {
            let ptr: *mut u8 = data.buf_ptr().cast();
            let len = data.len_bytes();

            let data = unsafe { std::slice::from_raw_parts(ptr, len) };
            self.0
                .spi_write(addr, data)
                .map_err(|v| PyException::new_err(v.to_string()))
        })
    }
}

#[pyclass]
pub struct UninitPciChip {
    pub chip: UninitChip,
}

#[pymethods]
impl UninitPciChip {
    pub fn init(&self) -> PyResult<PciChip> {
        Ok(PciChip(
            self.chip
                .clone()
                .init(&mut |_| {})
                .map_err(|v| PyException::new_err(v.to_string()))?,
        ))
    }
}

#[pyfunction]
#[pyo3(signature = (interfaces = None, local_only = false, continue_on_failure = false, chip_filter = None, noc_safe = false))]
pub fn detect_chips_fallible(
    interfaces: Option<Vec<usize>>,
    local_only: bool,
    continue_on_failure: bool,
    chip_filter: Option<Vec<String>>,
    noc_safe: bool,
) -> PyResult<Vec<UninitPciChip>> {
    let interfaces = interfaces.unwrap_or(Vec::new());

    let all_devices = luwen_ref::PciDevice::scan();

    let interfaces = if interfaces.len() == 0 {
        all_devices
    } else {
        let mut error_interfaces = Vec::with_capacity(interfaces.len());
        for interface in interfaces.iter().copied() {
            if !all_devices.contains(&interface) {
                error_interfaces.push(interface);
            }
        }

        if error_interfaces.len() > 0 {
            return Err(PyException::new_err(format!(
                "Could not open TT-PCI device: {:?}; expected one of {:?}",
                error_interfaces, interfaces
            )));
        }

        interfaces
    };

    let mut root_chips = Vec::with_capacity(interfaces.len());
    for interface in interfaces {
        root_chips.push(PciChip::new(Some(interface)).0);
    }

    let chip_filter = chip_filter.unwrap_or_default();
    let mut converted_chip_filter = Vec::with_capacity(chip_filter.len());
    for filter in chip_filter {
        converted_chip_filter.push(Arch::from_string(&filter).ok_or_else(|| {
            PyException::new_err(format!("Could not parse chip arch: {}", filter))
        })?);
    }

    let options = ChipDetectOptions {
        continue_on_failure,
        local_only,
        chip_filter: converted_chip_filter,
        noc_safe,
    };

    let chips = luwen_if::detect_chips(root_chips, &mut |_| {}, options)
        .map_err(|v| PyException::new_err(format!("Could perform dma transfer: {}", v)))?;
    Ok(chips
        .into_iter()
        .map(|chip| UninitPciChip { chip })
        .collect())
}

#[pyfunction]
#[pyo3(signature = (interfaces = None, local_only = false, continue_on_failure = false, chip_filter = None, noc_safe = false))]
pub fn detect_chips(
    interfaces: Option<Vec<usize>>,
    local_only: bool,
    continue_on_failure: bool,
    chip_filter: Option<Vec<String>>,
    noc_safe: bool,
) -> PyResult<Vec<PciChip>> {
    let chips = detect_chips_fallible(
        interfaces,
        local_only,
        continue_on_failure,
        chip_filter,
        noc_safe,
    )?;
    let mut output = Vec::with_capacity(chips.len());
    for chip in chips {
        output.push(chip.init().map_err(|v| PyException::new_err(v))?);
    }
    Ok(output)
}

#[pymodule]
fn pyluwen(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<PciChip>()?;
    m.add_class::<UninitPciChip>()?;
    m.add_class::<PciWormhole>()?;
    m.add_class::<RemoteWormhole>()?;
    m.add_class::<PciGrayskull>()?;
    m.add_class::<DmaBuffer>()?;
    m.add_class::<AxiData>()?;

    m.add_wrapped(wrap_pyfunction!(detect_chips))?;
    m.add_wrapped(wrap_pyfunction!(detect_chips_fallible))?;

    Ok(())
}
