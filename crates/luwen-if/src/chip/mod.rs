mod chip_comms;
mod chip_interface;
mod creation;
pub mod eth_addr;
mod grayskull;
mod hl_comms;
mod remote;
mod wormhole;

pub use chip_comms::{
    axi_translate, ArcIf, AxiData, AxiError, ChipComms, MemorySlice, MemorySlices,
};
pub use chip_interface::ChipInterface;
pub use grayskull::Grayskull;
pub use hl_comms::HlComms;
use luwen_core::Arch;
pub use wormhole::Wormhole;

use crate::{
    arc_msg::{ArcMsg, ArcMsgAddr, ArcMsgError, ArcMsgOk},
    error::PlatformError,
    DeviceInfo,
};

/// Arc message interface
#[derive(Debug)]
pub struct ArcMsgOptions {
    pub msg: ArcMsg,
    pub wait_for_done: bool,
    pub timeout: std::time::Duration,
    pub use_second_mailbox: bool,
    pub addrs: Option<ArcMsgAddr>,
}

impl Default for ArcMsgOptions {
    fn default() -> Self {
        Self {
            msg: ArcMsg::Nop,
            wait_for_done: true,
            timeout: std::time::Duration::from_secs(1),
            use_second_mailbox: false,
            addrs: None,
        }
    }
}

#[derive(Debug, Hash, PartialEq, Eq)]
pub struct NeighbouringChip {
    pub local_noc_addr: (u8, u8),
    pub remote_noc_addr: (u8, u8),
    pub eth_addr: crate::EthAddr,
}

pub struct Telemetry {
    pub board_id: u64,
}

/// Defines common functionality for all chips.
/// This is a convinence interface that allows chip type agnostic code to be written.
pub trait ChipImpl: Send + Sync + 'static {
    fn init(&self);

    fn get_arch(&self) -> Arch;

    fn get_telemetry(&self) -> Result<Telemetry, PlatformError>;

    fn arc_msg(&self, msg: ArcMsgOptions) -> Result<ArcMsgOk, ArcMsgError>;
    fn get_neighbouring_chips(&self) -> Vec<NeighbouringChip>;

    fn as_any(&self) -> &dyn std::any::Any;

    fn get_device_info(&self) -> Option<DeviceInfo>;
}

/// A wrapper around a chip that implements `ChipImpl`.
/// This allows us to create and use chips without knowing their type,
/// but we can still downcast to the concrete type if we need to.
pub struct Chip {
    pub inner: Box<dyn ChipImpl>,
}

impl From<Box<dyn ChipImpl>> for Chip {
    fn from(inner: Box<dyn ChipImpl>) -> Self {
        Self { inner }
    }
}

impl Chip {
    /// Downcast to a wormhole chip
    pub fn as_wh(&self) -> Option<&Wormhole> {
        self.inner.as_any().downcast_ref::<Wormhole>()
    }

    /// Downcast to a grayskull chip
    pub fn as_gs(&self) -> Option<&Grayskull> {
        self.inner.as_any().downcast_ref::<Grayskull>()
    }
}

impl ChipImpl for Chip {
    fn init(&self) {
        self.inner.init()
    }

    fn get_arch(&self) -> Arch {
        self.inner.get_arch()
    }

    fn arc_msg(&self, msg: ArcMsgOptions) -> Result<ArcMsgOk, ArcMsgError> {
        self.inner.arc_msg(msg)
    }

    fn get_neighbouring_chips(&self) -> Vec<NeighbouringChip> {
        self.inner.get_neighbouring_chips()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self.inner.as_any()
    }

    fn get_telemetry(&self) -> Result<Telemetry, PlatformError> {
        self.inner.get_telemetry()
    }

    fn get_device_info(&self) -> Option<DeviceInfo> {
        self.inner.get_device_info()
    }
}
