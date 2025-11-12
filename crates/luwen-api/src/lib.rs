// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

// Allow modules with the same name as their parent module as this improves organization of the codebase
#![allow(clippy::module_inception)]

/// Luwen-if implements all high level functions in a backend agnostic way.
/// In the simplest terms this includes everything defined in `ChipImpl`, `HlComms` and `detect_chips`.
/// But this also includes chip specific functions which can be found in `Wormhole` and `Grayskull` chips.
///
mod arc_msg;
pub mod chip;
mod detect_chips;
pub mod error;
mod interface;

pub use arc_msg::{
    ArcMsg, ArcMsgError, ArcMsgOk, ArcMsgProtocolError, ArcState, PowerState, TypedArcMsg,
};
pub use chip::eth_addr::{EthAddr, IntoChip};
pub use chip::ChipImpl;
pub use detect_chips::{detect_chips, detect_chips_silent, ChipDetectOptions, UninitChip};
pub use interface::{CallbackStorage, DeviceInfo, FnAxi, FnDriver, FnNoc, FnOptions, FnRemote};
