use std::ops::{Deref, DerefMut};

use luwen_if::chip::{HlComms, HlCommsInterface};
use luwen_if::{CallbackStorage, DeviceInfo};
use luwen_ref::{ExtendedPciDeviceWrapper, DmaConfig};
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

    pub fn init(&self) {
        self.0.inner.init();
    }

    pub fn board_id(&self) -> u64 {
        self.0.inner.get_telemetry().unwrap().board_id
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
        Ok(format!("{:02x}:{:02x}.{:x}", info.bus, info.slot, info.function))
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

    pub fn from_gs<'a>(wh: &'a PciGrayskull) -> Option<PciInterface<'a>> {
        wh.0.get_if::<CallbackStorage<ExtendedPciDeviceWrapper>>()
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

    pub fn allocate_dma_buffer(&self, size: u32) -> Result<(u64, u64), String> {
        let buffer = self
            .pci_interface
            .borrow_mut()
            .device
            .allocate_dma_buffer(size)
            .map_err(|v| v.to_string())?;
        Ok((buffer.buffer.as_ptr() as u64, buffer.physical_address))
    }

    pub fn config_dma(&self, csm_pcie_ctrl_dma_request_offset: u32, arc_misc_cntl_addr: u32, msi: bool, read_threshold: u32, write_threshold: u32) -> Result<(), String> {
        let borrow: &mut _ = &mut self.pci_interface.borrow_mut();
        borrow.device.dma_config = Some(DmaConfig {
            csm_pcie_ctrl_dma_request_offset,
            arc_misc_cntl_addr,
            dma_host_phys_addr_high: 0,
            support_64_bit_dma: false,
            use_msi_for_dma: msi,
            read_threshold,
            write_threshold,
        });

        Ok(())
    }

    pub fn dma_transfer_turbo(&self, addr: u32, physical_address: u64, size: u32, write: bool) -> Result<(), String> {
        let borrow: &mut _ = &mut self.pci_interface.borrow_mut();
        borrow.device
            .pcie_dma_transfer_turbo(addr, physical_address, size, write)
            .map_err(|v| v.to_string())
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

    pub fn allocate_dma_buffer(&self, size: u32) -> PyResult<(u64, u64)> {
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

    pub fn config_dma(&self, csm_pcie_ctrl_dma_request_offset: u32, arc_misc_cntl_addr: u32, msi: bool, read_threshold: u32, write_threshold: u32) -> PyResult<()> {
        let value = PciInterface::from_wh(self);

        if let Some(value) = value {
            Ok(value.config_dma(csm_pcie_ctrl_dma_request_offset, arc_misc_cntl_addr, msi, read_threshold, write_threshold).map_err(|v| {
                PyException::new_err(format!("Could perform dma config: {}", v))
            })?)
        } else {
            return Err(PyException::new_err(
                "Could not get PCI interface for this chip.",
            ));
        }
    }

    pub fn dma_transfer_turbo(&self, addr: u32, physical_dma_buffer: u64, size: u32, write: bool) -> PyResult<()> {
        let value = PciInterface::from_wh(self);

        if let Some(value) = value {
            Ok(value.dma_transfer_turbo(addr, physical_dma_buffer, size, write).map_err(|v| {
                PyException::new_err(format!("Could perform dma transfer: {}", v))
            })?)
        } else {
            return Err(PyException::new_err(
                "Could not get PCI interface for this chip.",
            ));
        }
    }
}

common_chip_comms_impls!(PciWormhole);

#[pyclass]
pub struct RemoteWormhole(luwen_if::chip::Wormhole);

common_chip_comms_impls!(RemoteWormhole);

#[pyfunction]
pub fn detect_chips() -> Vec<PciChip> {
    luwen_ref::detect_chips()
        .unwrap()
        .into_iter()
        .map(|chip| PciChip(chip))
        .collect()
}

#[pymodule]
fn pyluwen(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<PciChip>()?;
    m.add_class::<PciWormhole>()?;
    m.add_class::<RemoteWormhole>()?;
    m.add_class::<PciGrayskull>()?;
    m.add_class::<AxiData>()?;

    m.add_wrapped(wrap_pyfunction!(detect_chips))?;

    Ok(())
}
