// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

// Allow function definitions inside PyMethod macros that appear outside of modules where they're used
#![allow(non_local_definitions)]

use std::ops::{Deref, DerefMut};
use std::str::FromStr;

use luwen_core::Arch;
use luwen_if::chip::{
    ubb_wait_for_driver_load, wait_for_init, wh_ubb_ipmi_reset, ArcMsg, ArcMsgOk, ArcMsgOptions,
    ChipImpl, HlComms, HlCommsInterface, InitError, NocInterface,
};
use luwen_if::{CallbackStorage, ChipDetectOptions, DeviceInfo, UninitChip};
use luwen_ref::{DmaConfig, ExtendedPciDeviceWrapper};
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use serde_json::Value;
use std::collections::HashMap;
use ttkmd_if::PossibleTlbAllocation;

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
pub struct PciBlackhole(luwen_if::chip::Blackhole);

impl Deref for PciBlackhole {
    type Target = luwen_if::chip::Blackhole;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for PciBlackhole {
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
pub struct NeighbouringChip {
    #[pyo3(get)]
    local_noc_addr: (u8, u8),
    #[pyo3(get)]
    remote_noc_addr: (u8, u8),
    #[pyo3(get)]
    eth_addr: EthAddr,
}

impl From<luwen_if::chip::NeighbouringChip> for NeighbouringChip {
    fn from(value: luwen_if::chip::NeighbouringChip) -> Self {
        Self {
            local_noc_addr: value.local_noc_addr,
            remote_noc_addr: value.remote_noc_addr,
            eth_addr: value.eth_addr.into(),
        }
    }
}

#[pyclass]
pub struct Telemetry {
    #[pyo3(get)]
    board_id: u64,
    #[pyo3(get)]
    enum_version: u32,
    #[pyo3(get)]
    entry_count: u32,
    #[pyo3(get)]
    device_id: u32,
    #[pyo3(get)]
    asic_id: u32,
    #[pyo3(get)]
    asic_ro: u32,
    #[pyo3(get)]
    asic_idd: u32,
    #[pyo3(get)]
    board_id_high: u32,
    #[pyo3(get)]
    board_id_low: u32,
    #[pyo3(get)]
    arc0_fw_version: u32,
    #[pyo3(get)]
    arc1_fw_version: u32,
    #[pyo3(get)]
    arc2_fw_version: u32,
    #[pyo3(get)]
    arc3_fw_version: u32,
    #[pyo3(get)]
    spibootrom_fw_version: u32,
    #[pyo3(get)]
    eth_fw_version: u32,
    #[pyo3(get)]
    m3_bl_fw_version: u32,
    #[pyo3(get)]
    m3_app_fw_version: u32,
    #[pyo3(get)]
    ddr_speed: Option<u32>,
    #[pyo3(get)]
    ddr_status: u32,
    #[pyo3(get)]
    eth_status0: u32,
    #[pyo3(get)]
    eth_status1: u32,
    #[pyo3(get)]
    pcie_status: u32,
    #[pyo3(get)]
    faults: u32,
    #[pyo3(get)]
    arc0_health: u32,
    #[pyo3(get)]
    arc1_health: u32,
    #[pyo3(get)]
    arc2_health: u32,
    #[pyo3(get)]
    arc3_health: u32,
    #[pyo3(get)]
    fan_speed: u32,
    #[pyo3(get)]
    aiclk: u32,
    #[pyo3(get)]
    axiclk: u32,
    #[pyo3(get)]
    arcclk: u32,
    #[pyo3(get)]
    l2cpuclk0: u32,
    #[pyo3(get)]
    l2cpuclk1: u32,
    #[pyo3(get)]
    l2cpuclk2: u32,
    #[pyo3(get)]
    l2cpuclk3: u32,
    #[pyo3(get)]
    throttler: u32,
    #[pyo3(get)]
    vcore: u32,
    #[pyo3(get)]
    asic_temperature: u32,
    #[pyo3(get)]
    vreg_temperature: u32,
    #[pyo3(get)]
    board_temperature: u32,
    #[pyo3(get)]
    tdp: u32,
    #[pyo3(get)]
    tdc: u32,
    #[pyo3(get)]
    vdd_limits: u32,
    #[pyo3(get)]
    thm_limits: u32,
    #[pyo3(get)]
    wh_fw_date: u32,
    #[pyo3(get)]
    asic_tmon0: u32,
    #[pyo3(get)]
    asic_tmon1: u32,
    #[pyo3(get)]
    mvddq_power: u32,
    #[pyo3(get)]
    fw_bundle_version: u32,
    #[pyo3(get)]
    gddr_train_temp0: u32,
    #[pyo3(get)]
    gddr_train_temp1: u32,
    #[pyo3(get)]
    boot_date: u32,
    #[pyo3(get)]
    rt_seconds: u32,
    #[pyo3(get)]
    eth_debug_status0: u32,
    #[pyo3(get)]
    eth_debug_status1: u32,
    #[pyo3(get)]
    tt_flash_version: u32,
    #[pyo3(get)]
    timer_heartbeat: u32,
    #[pyo3(get)]
    noc_translation_enabled: bool,
    #[pyo3(get)]
    tensix_enabled_col: u32,
    #[pyo3(get)]
    enabled_eth: u32,
    #[pyo3(get)]
    enabled_gddr: u32,
    #[pyo3(get)]
    enabled_l2cpu: u32,
    #[pyo3(get)]
    enabled_pcie: u32,
    #[pyo3(get)]
    fan_rpm: u32,
    #[pyo3(get)]
    gddr01_temp: u32,
    #[pyo3(get)]
    gddr23_temp: u32,
    #[pyo3(get)]
    gddr45_temp: u32,
    #[pyo3(get)]
    gddr67_temp: u32,
    #[pyo3(get)]
    gddr01_corr_errs: u32,
    #[pyo3(get)]
    gddr23_corr_errs: u32,
    #[pyo3(get)]
    gddr45_corr_errs: u32,
    #[pyo3(get)]
    gddr67_corr_errs: u32,
    #[pyo3(get)]
    gddr_uncorr_errs: u32,
    #[pyo3(get)]
    max_gddr_temp: u32,
    #[pyo3(get)]
    asic_location: u32,
    #[pyo3(get)]
    board_power_limit: u32,
    #[pyo3(get)]
    input_power: u32,
    #[pyo3(get)]
    therm_trip_count: u32,
    #[pyo3(get)]
    asic_id_high: u32,
    #[pyo3(get)]
    asic_id_low: u32,
}
impl From<luwen_if::chip::Telemetry> for Telemetry {
    fn from(value: luwen_if::chip::Telemetry) -> Self {
        Self {
            board_id: value.board_id,
            enum_version: value.enum_version,
            entry_count: value.entry_count,
            device_id: value.device_id,
            asic_id: value.asic_id,
            asic_ro: value.asic_ro,
            asic_idd: value.asic_idd,
            board_id_high: value.board_id_high,
            board_id_low: value.board_id_low,
            arc0_fw_version: value.arc0_fw_version,
            arc1_fw_version: value.arc1_fw_version,
            arc2_fw_version: value.arc2_fw_version,
            arc3_fw_version: value.arc3_fw_version,
            spibootrom_fw_version: value.spibootrom_fw_version,
            eth_fw_version: value.eth_fw_version,
            m3_bl_fw_version: value.m3_bl_fw_version,
            m3_app_fw_version: value.m3_app_fw_version,
            ddr_speed: value.ddr_speed,
            ddr_status: value.ddr_status,
            eth_status0: value.eth_status0,
            eth_status1: value.eth_status1,
            pcie_status: value.pcie_status,
            faults: value.faults,
            arc0_health: value.arc0_health,
            arc1_health: value.arc1_health,
            arc2_health: value.arc2_health,
            arc3_health: value.arc3_health,
            fan_speed: value.fan_speed,
            aiclk: value.aiclk,
            axiclk: value.axiclk,
            arcclk: value.arcclk,
            l2cpuclk0: value.l2cpuclk0,
            l2cpuclk1: value.l2cpuclk1,
            l2cpuclk2: value.l2cpuclk2,
            l2cpuclk3: value.l2cpuclk3,
            throttler: value.throttler,
            vcore: value.vcore,
            asic_temperature: value.asic_temperature,
            vreg_temperature: value.vreg_temperature,
            board_temperature: value.board_temperature,
            tdp: value.tdp,
            tdc: value.tdc,
            vdd_limits: value.vdd_limits,
            thm_limits: value.thm_limits,
            wh_fw_date: value.wh_fw_date,
            asic_tmon0: value.asic_tmon0,
            asic_tmon1: value.asic_tmon1,
            mvddq_power: value.mvddq_power,
            gddr_train_temp0: value.gddr_train_temp0,
            gddr_train_temp1: value.gddr_train_temp1,
            boot_date: value.boot_date,
            rt_seconds: value.rt_seconds,
            eth_debug_status0: value.eth_debug_status0,
            eth_debug_status1: value.eth_debug_status1,
            tt_flash_version: value.tt_flash_version,
            fw_bundle_version: value.fw_bundle_version,
            timer_heartbeat: value.timer_heartbeat,
            noc_translation_enabled: value.noc_translation_enabled,
            tensix_enabled_col: value.tensix_enabled_col,
            enabled_eth: value.enabled_eth,
            enabled_gddr: value.enabled_gddr,
            enabled_l2cpu: value.enabled_l2cpu,
            enabled_pcie: value.enabled_pcie,
            fan_rpm: value.fan_rpm,
            gddr01_temp: value.gddr01_temp,
            gddr23_temp: value.gddr23_temp,
            gddr45_temp: value.gddr45_temp,
            gddr67_temp: value.gddr67_temp,
            gddr01_corr_errs: value.gddr01_corr_errs,
            gddr23_corr_errs: value.gddr23_corr_errs,
            gddr45_corr_errs: value.gddr45_corr_errs,
            gddr67_corr_errs: value.gddr67_corr_errs,
            gddr_uncorr_errs: value.gddr_uncorr_errs,
            max_gddr_temp: value.max_gddr_temp,
            asic_location: value.asic_location,
            board_power_limit: value.board_power_limit,
            input_power: value.input_power,
            therm_trip_count: value.therm_trip_count,
            asic_id_high: value.asic_id_high,
            asic_id_low: value.asic_id_low,
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

            pub fn noc_multicast(
                &self,
                noc_id: u8,
                start: (u8, u8),
                end: (u8, u8),
                addr: u64,
                data: pyo3::buffer::PyBuffer<u8>,
            ) -> PyResult<()> {
                Python::with_gil(|_py| {
                    let ptr: *mut u8 = data.buf_ptr().cast();
                    let len = data.len_bytes();

                    let data = unsafe { std::slice::from_raw_parts(ptr, len) };

                    let (start, end) = if self.0.get_arch() == Arch::Blackhole {
                        let telemetry = self.get_telemetry()?;
                        let translation_enabled = telemetry.noc_translation_enabled;

                         if translation_enabled && noc_id != 0{
                                (end, start)

                        }  else {
                            (start, end)
                        }
                    }  else {
                        (start, end)
                    };

                    self.0
                        .noc_multicast(noc_id, start, end, addr, data)
                        .map_err(|v| PyException::new_err(v.to_string()))
                })
            }

            pub fn noc_multicast32(&self, noc_id: u8, start: (u8, u8), end: (u8, u8), addr: u64, data: u32) -> PyResult<()> {
                self.0
                    .noc_multicast(noc_id, start, end, addr, &data.to_le_bytes())
                    .map_err(|v| PyException::new_err(v.to_string()))
            }

            fn broadcast_impl(&self, noc_id: u8, addr: u64, data: &[u8]) -> PyResult<()> {
                if self.0.get_arch() == Arch::Blackhole {
                    let telemetry = self.get_telemetry()?;
                    let translation_enabled = telemetry.noc_translation_enabled;

                    if translation_enabled {
                        let (start, end) = if noc_id == 0 {
                            ((2, 3), (1, 2))
                        }  else {
                            ((1, 2), (2, 3))
                        };

                        self.0
                            .noc_multicast(noc_id, start, end, addr, data)
                            .map_err(|v| PyException::new_err(v.to_string()))
                    }  else {
                        self.0
                            .noc_broadcast(noc_id, addr, data)
                            .map_err(|v| PyException::new_err(v.to_string()))
                    }
                } else {
                    self.0
                        .noc_broadcast(noc_id, addr, data)
                        .map_err(|v| PyException::new_err(v.to_string()))
                }
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
                    self.broadcast_impl(noc_id, addr, data)
                })
            }

            pub fn noc_broadcast32(&self, noc_id: u8, addr: u64, data: u32) -> PyResult<()> {
                self.broadcast_impl(noc_id, addr, &data.to_le_bytes())
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
            pub fn arc_msg(&self, msg: u16, wait_for_done: bool, use_second_mailbox: bool, arg0: u16, arg1: u16, timeout: f64) -> PyResult<Option<(u32, u32)>> {
                match self.0
                    .arc_msg(ArcMsgOptions {
                        addrs: None,
                        msg: ArcMsg::Raw{ msg, arg0, arg1 },
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

            pub fn get_telemetry(&self) -> PyResult<Telemetry> {
                self.0.get_telemetry().map(|v| v.into()).map_err(|v| PyException::new_err(v.to_string()))
            }

            pub fn get_neighbouring_chips(&self) -> PyResult<Vec<NeighbouringChip>> {
                self.0
                    .get_neighbouring_chips()
                    .map(|v| v.into_iter().map(|v| v.into()).collect())
                    .map_err(|v| PyException::new_err(v.to_string()))
            }

    }
}
}

#[pyclass]
struct PyChipDetectState(luwen_if::chip::ChipDetectState<'static>);

#[pymethods]
impl PyChipDetectState {
    pub fn new_chip(&self) -> bool {
        matches!(self.0.call, luwen_if::chip::CallReason::NewChip)
    }

    pub fn correct_down(&self) -> bool {
        matches!(self.0.call, luwen_if::chip::CallReason::NotNew)
    }

    pub fn status_string(&self) -> Option<String> {
        match self.0.call {
            luwen_if::chip::CallReason::NewChip | luwen_if::chip::CallReason::NotNew => None,
            luwen_if::chip::CallReason::ChipInitCompleted(status)
            | luwen_if::chip::CallReason::InitWait(status) => Some(format!(
                "{}\n{}\n{}",
                status.arc_status, status.dram_status, status.eth_status
            )),
        }
    }
}

impl PciChip {
    fn device_info(&self) -> PyResult<DeviceInfo> {
        match self.0.inner.get_device_info() {
            Ok(info) => {
                if let Some(info) = info {
                    Ok(info)
                } else {
                    Err(PyException::new_err(
                        "Could not get device info: info unavailable",
                    ))
                }
            }
            Err(err) => Err(PyException::new_err(format!(
                "Could not get device info: {err}"
            ))),
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

    pub fn as_bh(&self) -> Option<PciBlackhole> {
        self.0.as_bh().map(|v| PciBlackhole(v.clone()))
    }

    pub fn is_remote(&self) -> bool {
        if let Some(wh) = self.0.as_wh() {
            wh.is_remote
        } else {
            false
        }
    }

    #[new]
    pub fn new(pci_interface: Option<usize>) -> PyResult<Self> {
        let pci_interface = pci_interface.unwrap_or(0);

        let chip = luwen_ref::ExtendedPciDevice::open(pci_interface).map_err(|v| {
            PyException::new_err(format!(
                "Could not open chip on pci interface {pci_interface}\n Failed with: {v}"
            ))
        })?;

        let arch = chip.borrow().device.arch;

        Ok(PciChip(
            luwen_if::chip::Chip::open(
                arch,
                luwen_if::CallbackStorage {
                    callback: luwen_ref::comms_callback,
                    user_data: chip,
                },
            )
            .map_err(|v| PyException::new_err(format!("Could not initialize chip: {v}")))?,
        ))
    }

    #[pyo3(signature = (callback = None))]
    pub fn init(&mut self, callback: Option<PyObject>) -> PyResult<()> {
        #[allow(clippy::type_complexity)]
        let mut callback: Box<
            dyn FnMut(luwen_if::chip::ChipDetectState) -> Result<(), PyErr>,
        > = if let Some(callback) = callback {
            Box::new(move |status| {
                // Safety: This is extremly unsafe, the alternative would be to copy the status for
                // every invocation.
                let status = unsafe {
                    std::mem::transmute::<
                        luwen_if::chip::ChipDetectState<'_>,
                        luwen_if::chip::ChipDetectState<'_>,
                    >(status)
                };
                if let Err(err) =
                    Python::with_gil(|py| callback.call1(py, (PyChipDetectState(status),)))
                {
                    Err(err)
                } else {
                    Ok(())
                }
            })
        } else {
            Box::new(|_| Python::with_gil(|py| py.check_signals()))
        };

        match wait_for_init(&mut self.0, &mut callback, false, false) {
            Err(InitError::PlatformError(err)) => Err(PyException::new_err(format!(
                "Could not initialize chip: {err}"
            ))),
            Err(InitError::CallbackError(err)) => Err(err),
            Ok(status) => Ok(status),
        }?;

        Ok(())
    }

    pub fn board_id(&self) -> PyResult<u64> {
        Ok(self
            .0
            .inner
            .get_telemetry()
            .map_err(|v| {
                PyException::new_err(format!("Could not access chip telemetry, failing with {v}"))
            })?
            .board_id)
    }

    pub fn device_id(&self) -> PyResult<u32> {
        let info = self.device_info()?;
        Ok(((info.vendor as u32) << 16) | info.device_id as u32)
    }

    pub fn bar_size(&self) -> PyResult<u64> {
        let info = self.device_info()?;
        if let Some(bar_size) = info.bar_size {
            Ok(bar_size)
        } else {
            let device_id = if let Ok(device_id) = self.device_id() {
                device_id.to_string()
            } else {
                "?".to_string()
            };

            Err(PyException::new_err(format!(
                "Could not get bar_size from PciDevice[{device_id}]",
            )))
        }
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
    #[allow(clippy::too_many_arguments)]
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
            match ttkmd_if::tlb::Ordering::from(ordering) {
                ttkmd_if::tlb::Ordering::UNKNOWN(ordering) => Err(PyException::new_err(format!(
                    "Invalid ordering {ordering}."
                ))),
                ordering => value.setup_tlb(
                    index, addr, x_start, y_start, x_end, y_end, noc_sel, mcast, ordering, linked,
                ),
            }
        } else {
            Err(PyException::new_err(
                "Could not get PCI interface for this chip.",
            ))
        }
    }

    pub fn set_default_tlb(&self, index: u32) -> PyResult<()> {
        let value = PciInterface::from_gs(self);

        if let Some(value) = value {
            value.pci_interface.borrow_mut().default_tlb = PossibleTlbAllocation::Hardcoded(index);
            Ok(())
        } else {
            Err(PyException::new_err(
                "Could not get PCI interface for this chip.",
            ))
        }
    }

    pub fn pci_axi_read32(&self, addr: u32) -> PyResult<u32> {
        let value = PciInterface::from_gs(self);
        if let Some(value) = value {
            value
                .axi_read32(addr)
                .map_err(|v| PyException::new_err(v.to_string()))
        } else {
            Err(PyException::new_err(
                "Could not get PCI interface for this chip.",
            ))
        }
    }

    pub fn pci_axi_write32(&self, addr: u32, data: u32) -> PyResult<()> {
        let value = PciInterface::from_gs(self);
        if let Some(value) = value {
            value
                .axi_write32(addr, data)
                .map_err(|v| PyException::new_err(v.to_string()))
        } else {
            Err(PyException::new_err(
                "Could not get PCI interface for this chip.",
            ))
        }
    }

    pub fn pci_board_type(&self) -> PyResult<u16> {
        let value = PciInterface::from_gs(self);
        if let Some(value) = value {
            Ok(value.pci_interface.borrow().device.physical.subsystem_id)
        } else {
            Err(PyException::new_err(
                "Could not get PCI interface for this chip.",
            ))
        }
    }

    pub fn pci_interface_id(&self) -> PyResult<usize> {
        let value = PciInterface::from_gs(self);
        if let Some(value) = value {
            Ok(value.pci_interface.borrow().device.id)
        } else {
            Err(PyException::new_err(
                "Could not get PCI interface for this chip.",
            ))
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
    pub fn from_wh(wh: &PciWormhole) -> Option<PciInterface> {
        wh.0.get_if::<CallbackStorage<ExtendedPciDeviceWrapper>>()
            .map(|v| PciInterface {
                pci_interface: &v.user_data,
            })
    }

    pub fn from_gs(gs: &PciGrayskull) -> Option<PciInterface> {
        gs.0.get_if::<CallbackStorage<ExtendedPciDeviceWrapper>>()
            .map(|v| PciInterface {
                pci_interface: &v.user_data,
            })
    }

    pub fn from_bh(bh: &PciBlackhole) -> Option<PciInterface> {
        bh.0.get_if::<NocInterface>()
            .map(|v| &v.backing)
            .and_then(|v| {
                v.as_any()
                    .downcast_ref::<CallbackStorage<ExtendedPciDeviceWrapper>>()
            })
            .map(|v| PciInterface {
                pci_interface: &v.user_data,
            })
    }

    #[allow(clippy::too_many_arguments)]
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
        ordering: ttkmd_if::tlb::Ordering,
        linked: bool,
    ) -> PyResult<(u64, u64)> {
        self.pci_interface
            .borrow_mut()
            .device
            .setup_tlb(
                &PossibleTlbAllocation::Hardcoded(index),
                ttkmd_if::Tlb {
                    local_offset: addr,
                    x_end,
                    y_end,
                    x_start,
                    y_start,
                    noc_sel,
                    mcast,
                    ordering,
                    linked,
                    ..Default::default()
                },
            )
            .map_err(|v| PyException::new_err(v.to_string()))
    }

    pub fn noc_read(&self, tlb_index: u32, addr: u64, data: &mut [u8]) -> Result<(), String> {
        let index = PossibleTlbAllocation::Hardcoded(tlb_index);
        let mut tlb = self
            .pci_interface
            .borrow()
            .device
            .get_tlb(&index)
            .map_err(|v| v.to_string())?;
        tlb.local_offset = addr;

        self.pci_interface
            .borrow_mut()
            .device
            .noc_read(&index, tlb, data)
            .map_err(|v| v.to_string())?;

        Ok(())
    }

    pub fn noc_write(&self, tlb_index: u32, addr: u64, data: &[u8]) -> Result<(), String> {
        let index = PossibleTlbAllocation::Hardcoded(tlb_index);
        let mut tlb = self
            .pci_interface
            .borrow()
            .device
            .get_tlb(&index)
            .map_err(|v| v.to_string())?;
        tlb.local_offset = addr;

        self.pci_interface
            .borrow_mut()
            .device
            .noc_write(&index, tlb, data)
            .map_err(|v| v.to_string())?;

        Ok(())
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

#[pyclass]
#[derive(Clone)]
pub struct EthAddr {
    #[pyo3(get)]
    pub shelf_x: u8,
    #[pyo3(get)]
    pub shelf_y: u8,
    #[pyo3(get)]
    pub rack_x: u8,
    #[pyo3(get)]
    pub rack_y: u8,
}

impl From<luwen_if::EthAddr> for EthAddr {
    fn from(value: luwen_if::EthAddr) -> Self {
        Self {
            shelf_x: value.shelf_x,
            shelf_y: value.shelf_y,
            rack_x: value.rack_x,
            rack_y: value.rack_y,
        }
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
                .map_err(|v| PyException::new_err(format!("Could not open remote: {v}")))?,
        ))
    }

    #[allow(clippy::too_many_arguments)]
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
            match ttkmd_if::tlb::Ordering::from(ordering) {
                ttkmd_if::tlb::Ordering::UNKNOWN(ordering) => Err(PyException::new_err(format!(
                    "Invalid ordering {ordering}."
                ))),
                ordering => value.setup_tlb(
                    index, addr, x_start, y_start, x_end, y_end, noc_sel, mcast, ordering, linked,
                ),
            }
        } else {
            Err(PyException::new_err(
                "Could not get PCI interface for this chip.",
            ))
        }
    }

    pub fn set_default_tlb(&self, index: u32) -> PyResult<()> {
        let value = PciInterface::from_wh(self);

        if let Some(value) = value {
            value.pci_interface.borrow_mut().default_tlb = PossibleTlbAllocation::Hardcoded(index);
            Ok(())
        } else {
            Err(PyException::new_err(
                "Could not get PCI interface for this chip.",
            ))
        }
    }

    pub fn allocate_dma_buffer(&self, size: u32) -> PyResult<DmaBuffer> {
        let value = PciInterface::from_wh(self);

        if let Some(value) = value {
            Ok(value
                .allocate_dma_buffer(size)
                .map_err(|v| PyException::new_err(format!("Could not allocate DMA buffer: {v}")))?)
        } else {
            Err(PyException::new_err(
                "Could not get PCI interface for this chip.",
            ))
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
                .map_err(|v| PyException::new_err(format!("Could perform dma config: {v}")))?)
        } else {
            Err(PyException::new_err(
                "Could not get PCI interface for this chip.",
            ))
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
                .map_err(|v| PyException::new_err(format!("Could perform dma transfer: {v}")))?)
        } else {
            Err(PyException::new_err(
                "Could not get PCI interface for this chip.",
            ))
        }
    }

    pub fn pci_board_type(&self) -> PyResult<u16> {
        let value = PciInterface::from_wh(self);
        if let Some(value) = value {
            Ok(value.pci_interface.borrow().device.physical.subsystem_id)
        } else {
            Err(PyException::new_err(
                "Could not get PCI interface for this chip.",
            ))
        }
    }

    pub fn pci_interface_id(&self) -> PyResult<usize> {
        let value = PciInterface::from_wh(self);
        if let Some(value) = value {
            Ok(value.pci_interface.borrow().device.id)
        } else {
            Err(PyException::new_err(
                "Could not get PCI interface for this chip.",
            ))
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

    pub fn get_local_coord(&self) -> PyResult<EthAddr> {
        self.0
            .get_local_chip_coord()
            .map(|v| v.into())
            .map_err(|v| PyException::new_err(v.to_string()))
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

    pub fn get_local_coord(&self) -> PyResult<EthAddr> {
        self.0
            .get_local_chip_coord()
            .map(|v| v.into())
            .map_err(|v| PyException::new_err(v.to_string()))
    }
}

#[pymethods]
impl PciBlackhole {
    #[allow(clippy::too_many_arguments)]
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
        let value = PciInterface::from_bh(self);

        if let Some(value) = value {
            match ttkmd_if::tlb::Ordering::from(ordering) {
                ttkmd_if::tlb::Ordering::UNKNOWN(ordering) => Err(PyException::new_err(format!(
                    "Invalid ordering {ordering}."
                ))),
                ordering => value.setup_tlb(
                    index, addr, x_start, y_start, x_end, y_end, noc_sel, mcast, ordering, linked,
                ),
            }
        } else {
            Err(PyException::new_err(
                "Could not get PCI interface for this chip.",
            ))
        }
    }

    pub fn set_default_tlb(&self, index: u32) -> PyResult<()> {
        let value = PciInterface::from_bh(self);

        if let Some(value) = value {
            value.pci_interface.borrow_mut().default_tlb = PossibleTlbAllocation::Hardcoded(index);
            Ok(())
        } else {
            Err(PyException::new_err(
                "Could not get PCI interface for this chip.",
            ))
        }
    }

    pub fn allocate_dma_buffer(&self, size: u32) -> PyResult<DmaBuffer> {
        let value = PciInterface::from_bh(self);

        if let Some(value) = value {
            Ok(value
                .allocate_dma_buffer(size)
                .map_err(|v| PyException::new_err(format!("Could not allocate DMA buffer: {v}")))?)
        } else {
            Err(PyException::new_err(
                "Could not get PCI interface for this chip.",
            ))
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
        let value = PciInterface::from_bh(self);

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
                .map_err(|v| PyException::new_err(format!("Could perform dma config: {v}")))?)
        } else {
            Err(PyException::new_err(
                "Could not get PCI interface for this chip.",
            ))
        }
    }

    pub fn dma_transfer_turbo(
        &self,
        addr: u32,
        physical_dma_buffer: u64,
        size: u32,
        write: bool,
    ) -> PyResult<()> {
        let value = PciInterface::from_bh(self);

        if let Some(value) = value {
            Ok(value
                .dma_transfer_turbo(addr, physical_dma_buffer, size, write)
                .map_err(|v| PyException::new_err(format!("Could perform dma transfer: {v}")))?)
        } else {
            Err(PyException::new_err(
                "Could not get PCI interface for this chip.",
            ))
        }
    }

    pub fn pci_board_type(&self) -> PyResult<u16> {
        let value = PciInterface::from_bh(self);
        if let Some(value) = value {
            Ok(value.pci_interface.borrow().device.physical.subsystem_id)
        } else {
            Err(PyException::new_err(
                "Could not get PCI interface for this chip.",
            ))
        }
    }

    pub fn pci_interface_id(&self) -> PyResult<usize> {
        let value = PciInterface::from_bh(self);
        if let Some(value) = value {
            Ok(value.pci_interface.borrow().device.id)
        } else {
            Err(PyException::new_err(
                "Could not get PCI interface for this chip.",
            ))
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

    pub fn get_local_coord(&self) -> PyResult<EthAddr> {
        self.0
            .get_local_chip_coord()
            .map(|v| v.into())
            .map_err(|v| PyException::new_err(v.to_string()))
    }

    pub fn decode_boot_fs_table(&self, tag_name: &str) -> PyResult<Py<PyDict>> {
        // Deserialize the boot fs table given the tag name and return it as a pydict with the correct types
        Python::with_gil(|py| {
            let result = self
                .0
                .decode_boot_fs_table(tag_name)
                .map_err(|v| PyException::new_err(v.to_string()))?;
            let py_dict = PyDict::new(py);
            // Convert the HashMap<String, Value> to a pydict
            for (key, value) in result {
                let py_key: PyObject = key.into_py(py);
                let py_value: PyObject = serde_json_value_to_pyobject(py, &value)?;
                py_dict.set_item(py_key, py_value)?;
            }
            Ok(py_dict.into())
        })
    }

    pub fn encode_and_write_boot_fs_table(
        &self,
        py: Python,
        message: Py<PyDict>,
        tag_name: &str,
    ) -> PyResult<()> {
        // Convert the pydict to a HashMap<String, Value>
        let py_dict = message.as_ref(py);
        let mut result: HashMap<String, Value> = HashMap::new();

        for (key, value) in py_dict.iter() {
            let key: String = key.extract()?;
            let value: Value = pyobject_to_serde_json_value(py, value)?;
            result.insert(key, value);
        }

        // Encode the boot fs table given the tag name and write it to the spi
        self.0
            .encode_and_write_boot_fs_table(result, tag_name)
            .map_err(|v| PyException::new_err(v.to_string()))?;
        Ok(())
    }

    pub fn get_spirom_table_spi_addr(&self, tag_name: &str) -> PyResult<u32> {
        // Return the spi address given the tagname
        let result = self
            .0
            .get_boot_fs_tables_spi_read(tag_name)
            .map_err(|v| PyException::new_err(v.to_string()))?;
        if let Some(result) = result {
            Ok(result.1.spi_addr)
        } else {
            Err(PyException::new_err(format!(
                "Was not able to find {tag_name} in spirom table"
            )))
        }
    }

    pub fn get_spirom_table_image_size(&self, tag_name: &str) -> PyResult<u32> {
        // Return the spi image size when given the tagname
        let result = self
            .0
            .get_boot_fs_tables_spi_read(tag_name)
            .map_err(|v| PyException::new_err(v.to_string()))?;
        if let Some(result) = result {
            Ok(result.1.flags.image_size())
        } else {
            Err(PyException::new_err(format!(
                "Was not able to find {tag_name} in spirom table"
            )))
        }
    }
}

common_chip_comms_impls!(PciBlackhole);

#[pyclass]
pub struct UninitPciChip {
    pub chip: UninitChip,
}

#[pymethods]
impl UninitPciChip {
    pub fn init(&self) -> PyResult<PciChip> {
        let chip = self
            .chip
            .clone()
            .init(&mut |_| Python::with_gil(|py| py.check_signals()));
        match chip {
            Ok(chip) => Ok(PciChip(chip)),
            Err(InitError::PlatformError(err)) => Err(PyException::new_err(err.to_string())),
            Err(InitError::CallbackError(err)) => Err(err),
        }
    }

    pub fn have_comms(&self) -> bool {
        self.chip
            .status()
            .map(|v| !v.unknown_state && v.comms_status.ok())
            .unwrap_or(true)
    }

    pub fn force_upgrade(&self) -> PciChip {
        PciChip(self.chip.clone().upgrade())
    }

    pub fn dram_safe(&self) -> bool {
        self.chip.dram_safe()
    }

    pub fn eth_safe(&self) -> bool {
        self.chip.eth_safe()
    }

    pub fn arc_alive(&self) -> bool {
        self.chip.arc_alive()
    }

    pub fn cpu_safe(&self) -> bool {
        self.chip.cpu_safe()
    }
}

//silent callback (import), stdout (print)
//add arguments, (own or from luwen)
//from luwen, multiple points to different callback functions

#[pyfunction]
#[pyo3(signature = (interfaces = None, local_only = false, continue_on_failure = false, chip_filter = None, noc_safe = false, callback = None))]
pub fn detect_chips_fallible(
    interfaces: Option<Vec<usize>>,
    local_only: bool,
    continue_on_failure: bool,
    chip_filter: Option<Vec<String>>,
    noc_safe: bool,
    callback: Option<PyObject>,
) -> PyResult<Vec<UninitPciChip>> {
    let interfaces = interfaces.unwrap_or_default();

    let all_devices = luwen_ref::PciDevice::scan();
    let interfaces = if interfaces.is_empty() {
        all_devices
    } else {
        let mut error_interfaces = Vec::with_capacity(interfaces.len());
        for interface in interfaces.iter().copied() {
            if !all_devices.contains(&interface) {
                error_interfaces.push(interface);
            }
        }

        if !error_interfaces.is_empty() {
            return Err(PyException::new_err(format!(
                "Could not open TT-PCI device: {error_interfaces:?}; expected one of {all_devices:?}"
            )));
        }

        interfaces
    };

    let mut root_chips = Vec::with_capacity(interfaces.len());
    let mut failed_chips = Vec::with_capacity(interfaces.len());
    for interface in interfaces {
        let chip = PciChip::new(Some(interface))?.0;

        // First let's test basic pcie communication we may be in a hang state so it's
        // important that we let the detect function know

        // Hack(drosen): Basic init procedure should resolve this
        let scratch_0 = if chip.get_arch().is_blackhole() {
            "arc_ss.reset_unit.SCRATCH_0"
        } else {
            "ARC_RESET.SCRATCH[0]"
        };
        let result = chip.axi_sread32(scratch_0);
        if let Err(err) = result {
            // Basic comms have failed... we should output a nice error message on the console
            failed_chips.push((interface, chip, err));
        } else {
            root_chips.push(chip);
        }
    }

    let chip_filter = chip_filter.unwrap_or_default();
    let mut converted_chip_filter = Vec::with_capacity(chip_filter.len());
    for filter in chip_filter {
        converted_chip_filter.push(Arch::from_str(&filter).map_err(|value| {
            PyException::new_err(format!("Could not parse chip arch: {value}"))
        })?);
    }
    let options = ChipDetectOptions {
        continue_on_failure,
        local_only,
        chip_filter: converted_chip_filter,
        noc_safe,
    };

    #[allow(clippy::type_complexity)]
    let mut callback: Box<dyn FnMut(luwen_if::chip::ChipDetectState) -> Result<(), PyErr>> =
        if let Some(callback) = callback {
            Box::new(move |status| {
                // Safety: This is extremly unsafe, the alternative would be to copy the status for
                // every invocation.
                let status = unsafe {
                    std::mem::transmute::<
                        luwen_if::chip::ChipDetectState<'_>,
                        luwen_if::chip::ChipDetectState<'_>,
                    >(status)
                };
                if let Err(err) =
                    Python::with_gil(|py| callback.call1(py, (PyChipDetectState(status),)))
                {
                    Err(err)
                } else {
                    Ok(())
                }
            })
        } else {
            Box::new(|_| Python::with_gil(|py| py.check_signals()))
        };
    let mut chips = match luwen_if::detect_chips(root_chips, &mut callback, options) {
        Ok(chips) => chips,
        Err(InitError::PlatformError(err)) => {
            return Err(PyException::new_err(err.to_string()))?;
        }
        Err(InitError::CallbackError(err)) => {
            return Err(err)?;
        }
    };
    for (id, chip, err) in failed_chips.into_iter() {
        let mut status = luwen_if::chip::InitStatus::new_unknown();
        status.comms_status = luwen_if::chip::CommsStatus::CommunicationError(err.to_string());
        status.unknown_state = false;
        chips.insert(
            id,
            UninitChip::Partially {
                status: Box::new(status),
                underlying: chip,
            },
        );
    }

    Ok(chips
        .into_iter()
        .map(|chip| UninitPciChip { chip })
        .collect())
}

#[pyfunction]
#[pyo3(signature = (interfaces = None, local_only = false, continue_on_failure = false, chip_filter = None, noc_safe = false, callback = None))]
pub fn detect_chips(
    interfaces: Option<Vec<usize>>,
    local_only: bool,
    continue_on_failure: bool,
    chip_filter: Option<Vec<String>>,
    noc_safe: bool,
    callback: Option<PyObject>,
) -> PyResult<Vec<PciChip>> {
    let chips = detect_chips_fallible(
        interfaces,
        local_only,
        continue_on_failure,
        chip_filter,
        noc_safe,
        callback,
    )?;
    let mut output = Vec::with_capacity(chips.len());
    for chip in chips {
        output.push(chip.init()?);
    }
    Ok(output)
}

#[pyfunction]
pub fn pci_scan() -> Vec<usize> {
    luwen_ref::PciDevice::scan()
}

#[pyfunction]
pub fn run_wh_ubb_ipmi_reset(
    ubb_num: String,
    dev_num: String,
    op_mode: String,
    reset_time: String,
) -> PyResult<()> {
    wh_ubb_ipmi_reset(&ubb_num, &dev_num, &op_mode, &reset_time)
        .map_err(|v| PyException::new_err(v.to_string()))
}

#[pyfunction]
pub fn run_ubb_wait_for_driver_load() {
    ubb_wait_for_driver_load()
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
    m.add_class::<Telemetry>()?;

    m.add_class::<PciBlackhole>()?;

    m.add_wrapped(wrap_pyfunction!(detect_chips))?;
    m.add_wrapped(wrap_pyfunction!(detect_chips_fallible))?;
    m.add_wrapped(wrap_pyfunction!(pci_scan))?;
    m.add_wrapped(wrap_pyfunction!(run_wh_ubb_ipmi_reset))?;
    m.add_wrapped(wrap_pyfunction!(run_ubb_wait_for_driver_load))?;

    Ok(())
}

/// Helper function to convert serde_json::Value to PyObject
fn serde_json_value_to_pyobject(py: Python, value: &Value) -> PyResult<PyObject> {
    match value {
        Value::Null => Ok(py.None()),
        Value::Bool(b) => Ok(b.into_py(py)),
        Value::Number(n) => {
            if let Some(value) = n.as_i64() {
                Ok(value.into_py(py))
            } else if let Some(value) = n.as_u64() {
                Ok(value.into_py(py))
            } else if let Some(value) = n.as_f64() {
                Ok(value.into_py(py))
            } else {
                unimplemented!("No support for number of type {n}")
            }
        }
        Value::String(s) => Ok(s.into_py(py)),
        Value::Array(arr) => {
            // For the list we need to recursively convert each item
            let py_list: &PyList = PyList::empty(py);
            for item in arr {
                let py_item: PyObject = serde_json_value_to_pyobject(py, item)?;
                py_list.append(py_item)?;
            }
            Ok(py_list.into_py(py))
        }
        Value::Object(obj) => {
            // For the dict we need to recursively convert each key and value
            let py_dict: &PyDict = PyDict::new(py);
            for (key, value) in obj {
                let py_key: PyObject = key.into_py(py);
                let py_value: PyObject = serde_json_value_to_pyobject(py, value)?;
                py_dict.set_item(py_key, py_value)?;
            }
            Ok(py_dict.into_py(py))
        }
    }
}

/// Helper function to convert PyObject to serde_json::Value
fn pyobject_to_serde_json_value(_py: Python, obj: &PyAny) -> PyResult<Value> {
    if obj.is_none() {
        Ok(Value::Null)
    } else if let Ok(b) = obj.extract::<bool>() {
        Ok(Value::Bool(b))
    } else if let Ok(i) = obj.extract::<i64>() {
        Ok(Value::Number(i.into()))
    } else if let Ok(u) = obj.extract::<u64>() {
        Ok(Value::Number(u.into()))
    } else if let Ok(f) = obj.extract::<f64>() {
        Ok(Value::Number(serde_json::Number::from_f64(f).ok_or_else(
            || PyException::new_err("Failed to convert float to JSON number"),
        )?))
    } else if let Ok(s) = obj.extract::<String>() {
        Ok(Value::String(s))
    } else if let Ok(list) = obj.downcast::<PyList>() {
        let mut array = Vec::new();
        for item in list {
            array.push(pyobject_to_serde_json_value(_py, item)?);
        }
        Ok(Value::Array(array))
    } else if let Ok(dict) = obj.downcast::<PyDict>() {
        let mut map = serde_json::Map::new();
        for (key, value) in dict {
            let key: String = key.extract()?;
            map.insert(key, pyobject_to_serde_json_value(_py, value)?);
        }
        Ok(Value::Object(map))
    } else {
        Err(PyException::new_err("Unsupported Python object type"))
    }
}
