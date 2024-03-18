use luwen_if::{
    chip::{ArcMsgOptions, Chip, HlComms},
    ChipImpl,
};
use luwen_ref::detect_chips;

fn lds_reset(interfaces: &[usize]) -> Vec<Chip> {
    for interface in interfaces.iter().copied() {
        let fd = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(format!("/dev/tenstorrent/{interface}"))
            .unwrap();
        let mut reset_device = ttkmd_if::ioctl::ResetDevice {
            input: ttkmd_if::ioctl::ResetDeviceIn {
                flags: ttkmd_if::ioctl::RESET_DEVICE_RESET_PCIE_LINK,
                ..Default::default()
            },
            ..Default::default()
        };
        unsafe {
            ttkmd_if::ioctl::reset_device(std::os::fd::AsRawFd::as_raw_fd(&fd), &mut reset_device)
        }
        .unwrap();

        assert_eq!(reset_device.output.result, 0);
    }

    let mut output = Vec::new();
    for interface in interfaces.iter().copied() {
        output.push(luwen_ref::open(interface as usize).unwrap());
    }

    for chip in &output {
        chip.arc_msg(ArcMsgOptions {
            msg: luwen_if::TypedArcMsg::SetArcState {
                state: luwen_if::ArcState::A3,
            }
            .into(),
            ..Default::default()
        })
        .unwrap();
    }

    for chip in &output {
        chip.arc_msg(ArcMsgOptions {
            msg: luwen_if::TypedArcMsg::TriggerReset.into(),
            wait_for_done: false,
            ..Default::default()
        })
        .unwrap();
    }

    println!("Sleeping for 2 seconds to allow chip to come back online");
    std::thread::sleep(std::time::Duration::from_secs(2));

    for interface in interfaces {
        let fd = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(format!("/dev/tenstorrent/{interface}"))
            .unwrap();
        let mut reset_device = ttkmd_if::ioctl::ResetDevice {
            input: ttkmd_if::ioctl::ResetDeviceIn {
                flags: ttkmd_if::ioctl::RESET_DEVICE_RESTORE_STATE,
                ..Default::default()
            },
            ..Default::default()
        };
        unsafe {
            ttkmd_if::ioctl::reset_device(std::os::fd::AsRawFd::as_raw_fd(&fd), &mut reset_device)
        }
        .unwrap();

        assert_eq!(reset_device.output.result, 0);
    }

    output
}

fn main() {
    println!("STARTING RESET");

    let interfaces = luwen_ref::PciDevice::scan();
    for interface in interfaces.iter().copied() {
        luwen_ref::open(interface)
            .unwrap()
            .axi_write32(0x20000, 0xfaca)
            .unwrap();
    }

    for interface in interfaces.iter().copied() {
        let result = luwen_ref::open(interface)
            .unwrap()
            .axi_read32(0x20000)
            .unwrap();
        assert_eq!(result, 0xfaca);
    }

    let interfaces = luwen_ref::PciDevice::scan();
    lds_reset(&interfaces);

    for interface in interfaces.iter().copied() {
        let result = luwen_ref::open(interface)
            .unwrap()
            .axi_read32(0x20000)
            .unwrap();
        assert_ne!(result, 0xfaca);
    }

    println!("RESET COMPLETE");

    let chips = detect_chips().unwrap();

    for chip in chips {
        if let Some(wh) = chip.as_wh() {
            dbg!(wh.is_remote);
        }
        println!("{:016X}", chip.get_telemetry().unwrap().board_id);
    }
}
