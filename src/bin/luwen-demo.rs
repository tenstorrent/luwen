use luwen_if::{
    chip::{ArcMsgOptions, Chip},
    detect_chips, ArcMsg, CallbackStorage, ChipImpl,
};
use luwen_ref::{
    comms_callback, error::LuwenError, ExtendedPciDevice, ExtendedPciDeviceWrapper, PciDevice,
};

pub fn main() -> Result<(), LuwenError> {
    let mut chips = Vec::new();

    let device_ids = PciDevice::scan();
    for device_id in device_ids {
        let ud = ExtendedPciDevice::open(device_id)?;
        let arch = ud.borrow().device.arch;

        let chip = Chip::open(arch, CallbackStorage::new(comms_callback, ud));

        if let Some(wh) = chip.as_wh() {
            let hi = wh
                .chip_if
                .as_any()
                .downcast_ref::<CallbackStorage<ExtendedPciDeviceWrapper>>()
                .unwrap();
        }
    }

    let device_ids = PciDevice::scan();
    for device_id in device_ids {
        println!("Running on device {device_id}");
        let ud = ExtendedPciDevice::open(device_id)?;
        let arch = ud.borrow().device.arch;

        let chip = Chip::open(arch, CallbackStorage::new(comms_callback, ud));

        chip.arc_msg(ArcMsgOptions {
            msg: ArcMsg::Test { arg: 101 },
            ..Default::default()
        })?;

        if let Some(wh) = chip.as_wh() {
            let remote_wh = wh.open_remote((1, 0)).unwrap();

            remote_wh.arc_msg(ArcMsgOptions {
                msg: ArcMsg::Test { arg: 101 },
                ..Default::default()
            })?;
        }

        chips.push(chip);
    }

    let all_chips = detect_chips(chips)?;
    for (chip_id, chip) in all_chips.into_iter().enumerate() {
        println!("Running on device {chip_id}");
        chip.arc_msg(ArcMsgOptions {
            msg: ArcMsg::Test { arg: 101 },
            ..Default::default()
        })?;
    }

    Ok(())
}
