use kmdif::PciDevice;
use luwen_if::{chip::Chip, CallbackStorage};

use crate::{comms_callback, error::LuwenError, ExtendedPciDevice};

pub fn detect_chips() -> Result<Vec<Chip>, LuwenError> {
    let mut chips = Vec::new();

    let device_ids = PciDevice::scan();
    for device_id in device_ids {
        let ud = ExtendedPciDevice::open(device_id)?;

        let arch = ud.borrow().device.arch;

        chips.push(Chip::open(
            arch,
            CallbackStorage::new(comms_callback, ud.clone()),
        ));
    }

    Ok(luwen_if::detect_chips(chips)?)
}
