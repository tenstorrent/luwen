use std::{
    any::TypeId,
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

pub use luwen_core;
pub use luwen_if;
pub use luwen_ref;
pub use ttkmd_if;

pub struct ModuleRegistery {
    registry: BTreeMap<TypeId, Box<dyn std::any::Any>>,
}

impl ModuleRegistery {
    // 'static means lives for the lifetime of the program. Applies to all type deglared in the static context
    pub fn set<T: 'static>(&mut self, value: T) -> Option<Box<T>> {
        if let Some(value) = self.registry.insert(TypeId::of::<T>(), Box::new(value)) {
            value.downcast().ok()
        } else {
            None
        }
    }

    pub fn has<T: 'static>(&self) -> bool {
        self.registry.contains_key(&TypeId::of::<T>())
    }

    pub fn get<T: 'static>(&self) -> Option<&T> {
        self.registry
            .get(&TypeId::of::<T>())
            .map(|v| v.downcast_ref())
            .flatten()
    }

    pub fn get_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.registry
            .get_mut(&TypeId::of::<T>())
            .map(|v| v.downcast_mut())
            .flatten()
    }
}

pub trait Tile: 'static {
    fn read(&self, addr: u32);
    fn write(&self, addr: u32, value: u32);
}

pub enum Chip {
    Blackhole(Blackhole),
    Wormhole(Wormhole),
    Grayskull(Grayskull),
}

impl Chip {
    pub fn get<T: Tile>(&self) -> Option<&T> {
        match self {
            Chip::Blackhole(blackhole) => blackhole.get(),
            Chip::Wormhole(wormhole) => wormhole.get(),
            Chip::Grayskull(grayskull) => grayskull.get(),
        }
    }

    pub fn get_mut<T: Tile>(&mut self) -> Option<&mut T> {
        match self {
            Chip::Blackhole(blackhole) => blackhole.get_mut(),
            Chip::Wormhole(wormhole) => wormhole.get_mut(),
            Chip::Grayskull(grayskull) => grayskull.get_mut(),
        }
    }

    /// Downcast to a wormhole chip
    pub fn as_wh(&self) -> Option<&Wormhole> {
        if let Self::Wormhole(chip) = self {
            Some(chip)
        } else {
            None
        }
    }

    /// Downcast to a grayskull chip
    pub fn as_gs(&self) -> Option<&Grayskull> {
        if let Self::Grayskull(chip) = self {
            Some(chip)
        } else {
            None
        }
    }

    /// Downcast to a blackhole chip
    pub fn as_bh(&self) -> Option<&Blackhole> {
        if let Self::Blackhole(chip) = self {
            Some(chip)
        } else {
            None
        }
    }
}

trait ChipImpl: Sized + 'static {
    /// Access underlying components
    fn get<T: Tile>(&self) -> Option<&T>;
    fn get_mut<T: Tile>(&mut self) -> Option<&mut T>;
}

pub struct DRAM {
    manager: PciDeviceManger,
    coord: (u8, u8),
}

impl Tile for DRAM {
    fn read(&self, addr: u32) {
        todo!()
    }

    fn write(&self, addr: u32, value: u32) {
        todo!()
    }
}

pub struct ARC {
    manager: PciDeviceManger,
    coord: (u8, u8),
}
impl Tile for ARC {
    fn read(&self, addr: u32) {
        self.manager.program_tlb(ttkmd_if::Tlb {
            x_end: self.coord.0,
            y_end: self.coord.1,
            local_offset: addr as u64,
            ..Default::default()
        });
        self.manager.tlb_read32();
    }

    fn write(&self, addr: u32, value: u32) {
        todo!()
    }
}

trait NocApi {
    fn program_tlb(&self, tlb: ttkmd_if::Tlb);

    fn tlb_write(&self);
    fn tlb_write32(&self);
    fn tlb_read(&self);
    fn tlb_read32(&self);
}

#[derive(Clone)]
pub struct PciDeviceManger {
    device: Arc<Mutex<ttkmd_if::PciDevice>>,
}

impl PciDeviceManger {
    pub fn open(id: usize) -> Result<Self, ttkmd_if::PciOpenError> {
        let device = ttkmd_if::PciDevice::open(id)?;

        Ok(Self {
            device: Arc::new(Mutex::new(device)),
        })
    }
}

impl NocApi for PciDeviceManger {
    fn program_tlb(&self, tlb: ttkmd_if::Tlb) {
        todo!()
    }

    fn tlb_write(&self) {
        todo!()
    }

    fn tlb_write32(&self) {
        todo!()
    }

    fn tlb_read(&self) {
        todo!()
    }

    fn tlb_read32(&self) {
        todo!()
    }
}

pub struct Blackhole {
    dram: Vec<DRAM>,
    arc: ARC,
}

impl Blackhole {
    pub fn open(id: usize) -> Self {
        let manager = PciDeviceManger::open(id).unwrap();

        Blackhole {
            dram: vec![DRAM {
                manager: manager.clone(),
                coord: (0, 0),
            }],
            arc: ARC {
                manager: manager.clone(),
                coord: (0, 8),
            },
        }
    }
}

impl ChipImpl for Blackhole {
    fn get<T: Tile>(&self) -> Option<&T> {
        match TypeId::of::<T>() {
            id if id == TypeId::of::<Vec<DRAM>>() => unsafe {
                ((&self.dram) as *const Vec<DRAM> as *const T).as_ref()
            },
            id if id == TypeId::of::<ARC>() => unsafe {
                ((&self.arc) as *const ARC as *const T).as_ref()
            },
            _ => None,
        }
    }

    fn get_mut<T: Tile>(&mut self) -> Option<&mut T> {
        match TypeId::of::<T>() {
            id if id == TypeId::of::<Vec<DRAM>>() => unsafe {
                ((&mut self.dram) as *mut Vec<DRAM> as *mut T).as_mut()
            },
            id if id == TypeId::of::<ARC>() => unsafe {
                ((&mut self.arc) as *mut ARC as *mut T).as_mut()
            },
            _ => None,
        }
    }
}

pub struct Wormhole {}

impl ChipImpl for Wormhole {
    fn get<T: Tile>(&self) -> Option<&T> {
        todo!()
    }

    fn get_mut<T: Tile>(&mut self) -> Option<&mut T> {
        todo!()
    }
}

pub struct Grayskull {}

impl ChipImpl for Grayskull {
    fn get<T: Tile>(&self) -> Option<&T> {
        todo!()
    }

    fn get_mut<T: Tile>(&mut self) -> Option<&mut T> {
        todo!()
    }
}
