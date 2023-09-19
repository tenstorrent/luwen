use std::ops::{Deref, DerefMut};

use luwen_if::EthAddr;
use pyo3::prelude::*;

#[pyclass]
pub struct Chip(luwen_if::chip::Chip);

impl Deref for Chip {
    type Target = luwen_if::chip::Chip;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Chip {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[pyclass]
pub struct Wormhole(luwen_if::chip::Wormhole);

impl Deref for Wormhole {
    type Target = luwen_if::chip::Wormhole;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Wormhole {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[pyclass]
pub struct Grayskull(luwen_if::chip::Grayskull);

impl Deref for Grayskull {
    type Target = luwen_if::chip::Grayskull;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Grayskull {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[pymethods]
impl Chip {
    pub fn as_wh(&self) -> Option<Wormhole> {
        self.0.as_wh().map(|v| Wormhole(v.clone()))
    }

    pub fn as_gs(&self) -> Option<Grayskull> {
        self.0.as_gs().map(|v| Grayskull(v.clone()))
    }

    #[new]
    pub fn new(pci_interface: usize) -> Self {
        let chip = luwen_ref::ExtendedPciDevice::open(pci_interface).unwrap();

        let arch = chip.borrow().device.arch;

        Chip(luwen_if::chip::Chip::open(
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
}

#[pymethods]
impl Wormhole {
    pub fn open_remote(
        &self,
        rack_x: Option<u8>,
        rack_y: Option<u8>,
        shelf_x: Option<u8>,
        shelf_y: Option<u8>,
    ) -> Self {
        Wormhole(
            self.0
                .open_remote((rack_x, rack_y, shelf_x, shelf_y))
                .unwrap(),
        )
    }
}

#[pyfunction]
pub fn detect_chips() -> Vec<Chip> {
    luwen_ref::detect_chips()
        .unwrap()
        .into_iter()
        .map(|chip| Chip(chip))
        .collect()
}

#[pymodule]
fn pyluwen(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<Chip>()?;
    m.add_class::<Wormhole>()?;
    m.add_class::<Grayskull>()?;

    m.add_wrapped(wrap_pyfunction!(detect_chips))?;

    Ok(())
}
