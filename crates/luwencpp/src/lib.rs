// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use luwen_if::{
    chip::{ArcMsgOptions, Chip},
    error::PlatformError,
    ArcMsg, ArcMsgError, ArcMsgProtocolError, CallbackStorage, ChipImpl, FnOptions,
};

#[repr(C)]
pub struct EthAddr {
    shelf_x: u8,
    shelf_y: u8,
    rack_x: u8,
    rack_y: u8,
}

impl From<EthAddr> for luwen_if::EthAddr {
    fn from(value: EthAddr) -> Self {
        luwen_if::EthAddr {
            shelf_x: value.shelf_x,
            shelf_y: value.shelf_y,
            rack_x: value.rack_x,
            rack_y: value.rack_y,
        }
    }
}

#[repr(u8)]
pub enum Arch {
    GRAYSKULL,
    WORMHOLE,
}

#[repr(C)]
pub struct DeviceInfo {
    pub interface_id: u32,

    pub domain: u16,
    pub bus: u16,
    pub slot: u16,
    pub function: u16,

    pub vendor: u16,
    pub device_id: u16,
    pub bar_size: u64,
    pub board_id: u16,
}

impl From<DeviceInfo> for luwen_if::DeviceInfo {
    fn from(value: DeviceInfo) -> Self {
        luwen_if::DeviceInfo {
            interface_id: value.interface_id,
            domain: value.domain,
            bus: value.bus,
            slot: value.slot,
            function: value.function,
            vendor: value.vendor,
            device_id: value.device_id,
            bar_size: value.bar_size,
            board_id: value.board_id,
        }
    }
}

#[repr(C)]
#[derive(Clone)]
pub struct LuwenGlue {
    user_data: *mut std::ffi::c_void,

    device_info: extern "C" fn(user_data: *mut std::ffi::c_void) -> DeviceInfo,

    /// Impls for bar reads and writes, the lowest level of communication
    /// used by local chips to talk to ARC.
    axi_read: extern "C" fn(addr: u32, data: *mut u8, len: u32, user_data: *mut std::ffi::c_void),
    axi_write:
        extern "C" fn(addr: u32, data: *const u8, len: u32, user_data: *mut std::ffi::c_void),

    /// Impls for noc reads and writes
    noc_read: extern "C" fn(
        noc_id: u8,
        x: u32,
        y: u32,
        addr: u64,
        data: *mut u8,
        len: u64,
        user_data: *mut std::ffi::c_void,
    ),
    noc_write: extern "C" fn(
        noc_id: u8,
        x: u32,
        y: u32,
        addr: u64,
        data: *const u8,
        len: u64,
        user_data: *mut std::ffi::c_void,
    ),
    noc_broadcast: extern "C" fn(
        noc_id: u8,
        addr: u64,
        data: *const u8,
        len: u64,
        user_data: *mut std::ffi::c_void,
    ),

    /// Impls for eth reads and writes could be implemented with noc operations but
    /// requires exclusive access to the erisc being used. Managing this is left to the implementor.
    eth_read: extern "C" fn(
        eth_addr: EthAddr,
        noc_id: u8,
        x: u32,
        y: u32,
        addr: u64,
        data: *mut u8,
        len: u64,
        user_data: *mut std::ffi::c_void,
    ),
    eth_write: extern "C" fn(
        eth_addr: EthAddr,
        noc_id: u8,
        x: u32,
        y: u32,
        addr: u64,
        data: *const u8,
        len: u64,
        user_data: *mut std::ffi::c_void,
    ),
    eth_broadcast: extern "C" fn(
        eth_addr: EthAddr,
        noc_id: u8,
        addr: u64,
        data: *const u8,
        len: u64,
        user_data: *mut std::ffi::c_void,
    ),
}

/// SAFETY: I really don't have a way to guarantee that the user_data is safe to send to other threads
/// but luwen won't create any threads so we are relying on the user to not create threads if it would be
/// unsafe for the user_data to be sent to other threads.
unsafe impl Send for LuwenGlue {}
unsafe impl Sync for LuwenGlue {}

pub fn callback_glue(
    glue_data: &LuwenGlue,
    options: FnOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    match options {
        FnOptions::Driver(op) => match op {
            luwen_if::FnDriver::DeviceInfo(info) => {
                let option = (glue_data.device_info)(glue_data.user_data);
                unsafe {
                    *info = Some(option.into());
                }
                Ok(())
            }
        },
        FnOptions::Axi(op) => match op {
            luwen_if::FnAxi::Read { addr, data, len } => {
                (glue_data.axi_read)(addr, data, len, glue_data.user_data);
                Ok(())
            }
            luwen_if::FnAxi::Write { addr, data, len } => {
                (glue_data.axi_write)(addr, data, len, glue_data.user_data);
                Ok(())
            }
        },
        FnOptions::Noc(op) => match op {
            luwen_if::FnNoc::Read {
                noc_id,
                x,
                y,
                addr,
                data,
                len,
            } => {
                (glue_data.noc_read)(noc_id, x, y, addr, data, len, glue_data.user_data);
                Ok(())
            }
            luwen_if::FnNoc::Write {
                noc_id,
                x,
                y,
                addr,
                data,
                len,
            } => {
                (glue_data.noc_write)(noc_id, x, y, addr, data, len, glue_data.user_data);
                Ok(())
            }
            luwen_if::FnNoc::Broadcast {
                noc_id,
                addr,
                data,
                len,
            } => {
                (glue_data.noc_broadcast)(noc_id, addr, data, len, glue_data.user_data);
                Ok(())
            }
        },
        FnOptions::Eth(op) => match op.rw {
            luwen_if::FnNoc::Read {
                noc_id,
                x,
                y,
                addr,
                data,
                len,
            } => {
                (glue_data.eth_read)(
                    EthAddr {
                        shelf_x: op.addr.shelf_x,
                        shelf_y: op.addr.shelf_y,
                        rack_x: op.addr.rack_x,
                        rack_y: op.addr.rack_y,
                    },
                    noc_id,
                    x,
                    y,
                    addr,
                    data,
                    len,
                    glue_data.user_data,
                );
                Ok(())
            }
            luwen_if::FnNoc::Write {
                noc_id,
                x,
                y,
                addr,
                data,
                len,
            } => {
                (glue_data.eth_write)(
                    EthAddr {
                        shelf_x: op.addr.shelf_x,
                        shelf_y: op.addr.shelf_y,
                        rack_x: op.addr.rack_x,
                        rack_y: op.addr.rack_y,
                    },
                    noc_id,
                    x,
                    y,
                    addr,
                    data,
                    len,
                    glue_data.user_data,
                );
                Ok(())
            }
            luwen_if::FnNoc::Broadcast {
                noc_id,
                addr,
                data,
                len,
            } => {
                (glue_data.eth_broadcast)(
                    EthAddr {
                        shelf_x: op.addr.shelf_x,
                        shelf_y: op.addr.shelf_y,
                        rack_x: op.addr.rack_x,
                        rack_y: op.addr.rack_y,
                    },
                    noc_id,
                    addr,
                    data,
                    len,
                    glue_data.user_data,
                );
                Ok(())
            }
        },
    }
}

#[no_mangle]
pub extern "C" fn luwen_open(arch: Arch, glue: LuwenGlue) -> *mut Chip {
    let arch = match arch {
        Arch::GRAYSKULL => luwen_core::Arch::Grayskull,
        Arch::WORMHOLE => luwen_core::Arch::Wormhole,
    };

    if let Ok(chip) = Chip::open(
        arch,
        CallbackStorage {
            callback: callback_glue,
            user_data: glue,
        },
    ) {
        Box::leak(Box::new(chip))
    } else {
        std::ptr::null_mut()
    }
}

#[no_mangle]
/// # Safety
///
/// local_chip must be a valid pointer
pub unsafe extern "C" fn luwen_open_remote(local_chip: *mut Chip, addr: EthAddr) -> *mut Chip {
    let local_chip = unsafe { &*local_chip };

    if let Some(wh) = local_chip.as_wh() {
        let remote = wh.open_remote(luwen_if::EthAddr::from(addr)).unwrap();
        Box::leak(Box::new(Chip::from(Box::new(remote) as Box<_>)))
    } else {
        std::ptr::null_mut()
    }
}

#[no_mangle]
/// # Safety
///
/// chip must be a valid pointer
pub unsafe extern "C" fn luwen_close(chip: *mut Chip) {
    unsafe {
        let _ = Box::from_raw(chip);
    }
}

#[repr(u8)]
pub enum CResultTag {
    Ok,
    Err,
}

#[repr(C)]
pub struct CResult {
    pub tag: CResultTag,
    pub ok: u32,
    pub err: *const std::ffi::c_char,
}

impl CResult {
    pub fn ok(value: u32) -> CResult {
        CResult {
            tag: CResultTag::Ok,
            ok: value,
            err: std::ptr::null(),
        }
    }

    pub fn err(value: &str) -> CResult {
        CResult {
            tag: CResultTag::Err,
            ok: 0,
            err: std::ffi::CString::new(value).unwrap().into_raw(),
        }
    }
}

#[allow(clippy::not_unsafe_ptr_arg_deref)] // The pointer is allowed to be null
#[no_mangle]
pub extern "C" fn chip_arc_msg(
    chip: &Chip,
    msg: u32,
    wait_for_done: bool,
    arg0: u16,
    arg1: u16,
    timeout: i32,
    return_3: *mut u32,
) -> CResult {
    match chip.arc_msg(ArcMsgOptions {
        msg: ArcMsg::from_values(msg, arg0, arg1),
        wait_for_done,
        timeout: std::time::Duration::from_secs(timeout as u64),
        ..Default::default()
    }) {
        Ok(value) => match value {
            luwen_if::ArcMsgOk::Ok { rc, arg } => {
                if !return_3.is_null() {
                    unsafe {
                        *return_3 = arg;
                    }
                }
                CResult::ok(rc)
            }
            luwen_if::ArcMsgOk::OkNoWait => CResult::ok(0),
        },
        Err(err) => {
            if let PlatformError::ArcMsgError(ArcMsgError::ProtocolError {
                source: ArcMsgProtocolError::MsgNotRecognized(arc),
                ..
            }) = err
            {
                CResult::ok(arc as u32)
            } else {
                CResult::err(&err.to_string())
            }
        }
    }
}

#[repr(C)]
pub struct Telemetry {
    board_id: u64,
}

impl From<luwen_if::chip::Telemetry> for Telemetry {
    fn from(value: luwen_if::chip::Telemetry) -> Self {
        Telemetry {
            board_id: value.board_id,
        }
    }
}

#[no_mangle]
pub extern "C" fn chip_telemetry(chip: &Chip) -> Telemetry {
    chip.get_telemetry().unwrap().into()
}
