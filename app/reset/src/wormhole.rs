use luwen_if::{chip::ArcMsgOptions, ChipImpl};

use crate::Reset;

pub struct ResetTracker {
    interface: usize,
}

impl ResetTracker {
    pub fn init(interface: usize) -> Self {
        Self { interface }
    }
}

impl Reset for ResetTracker {
    fn reset(&mut self) {
        let chip = luwen_ref::open(self.interface).unwrap();

        chip.arc_msg(ArcMsgOptions {
            msg: luwen_if::TypedArcMsg::SetArcState {
                state: luwen_if::ArcState::A3,
            }
            .into(),
            ..Default::default()
        })
        .unwrap();
        chip.arc_msg(ArcMsgOptions {
            msg: luwen_if::TypedArcMsg::TriggerReset.into(),
            wait_for_done: false,
            ..Default::default()
        })
        .unwrap();
    }

    fn wait(&mut self) -> bool {
        false
    }

    fn restore(&mut self) {
        let fd = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(format!("/dev/tenstorrent/{}", self.interface))
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
}
