mod communication;
mod creation;
pub mod eth_addr;
mod grayskull;
mod hl_comms;
mod init;
mod remote;
mod wormhole;

pub use communication::chip_comms::{
    axi_translate, ArcIf, AxiData, AxiError, ChipComms, MemorySlice, MemorySlices,
};
pub use communication::chip_interface::ChipInterface;
pub use grayskull::Grayskull;
pub use hl_comms::{HlComms, HlCommsInterface};
pub use init::{wait_for_init, CallReason, ChipDetectState};
use luwen_core::Arch;
pub use wormhole::Wormhole;

use crate::{arc_msg::ArcMsgAddr, error::PlatformError, DeviceInfo};

pub use crate::arc_msg::{ArcMsg, ArcMsgOk};

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

pub enum ArcStatus {
    ArcError(String),
    DramTraining,
    ArcOk,
}

#[derive(Debug)]
pub enum WaitStatus {
    Waiting(std::time::Instant),
    Timeout(std::time::Duration),
    JustFinished,
    Done,
    NotPresent,
}

impl WaitStatus {
    pub fn is_done(&self) -> bool {
        match self {
            Self::Done => true,
            _ => false,
        }
    }
}

impl Default for WaitStatus {
    fn default() -> Self {
        Self::NotPresent
    }
}

#[derive(Default, Debug)]
pub struct StatusInfo {
    pub ready: usize,
    pub total: usize,
    pub wait_status: WaitStatus,
    pub status: String,
}

impl StatusInfo {
    pub fn not_present() -> Self {
        Self {
            wait_status: WaitStatus::NotPresent,
            ..Default::default()
        }
    }

    pub fn get_status(&self) -> String {
        match &self.wait_status {
            WaitStatus::Waiting(_) => {
                let status_line = if !self.status.is_empty() {
                    format!(": {}", self.status)
                } else {
                    format!("")
                };

                let total_line = if self.total > 0 {
                    format!("[{}/{}]", self.ready, self.total)
                } else {
                    format!("")
                };

                format!("{total_line}{status_line}")
            }
            WaitStatus::Timeout(timeout) => {
                let status_line = if !self.status.is_empty() {
                    format!(": {}", self.status)
                } else {
                    format!("")
                };

                let timeout_line = format!("Timeout after {}", timeout.as_secs());

                format!("{timeout_line}{status_line}")
            }
            WaitStatus::JustFinished | WaitStatus::Done => {
                let status_line = if !self.status.is_empty() {
                    format!(": {}", self.status)
                } else {
                    format!("")
                };

                format!("Completed{status_line}")
            }
            WaitStatus::NotPresent => String::from("No Present"),
        }
    }

    pub fn is_waiting(&self) -> bool {
        match &self.wait_status {
            WaitStatus::Waiting(_) => true,
            _ => false,
        }
    }

    pub fn is_error(&self) -> bool {
        match &self.wait_status {
            WaitStatus::Timeout(_) => true,
            WaitStatus::JustFinished | WaitStatus::Done => self.ready != self.total,
            _ => false,
        }
    }

    pub fn is_completed(&self) -> bool {
        match &self.wait_status {
            WaitStatus::Timeout(_) | WaitStatus::Done | WaitStatus::JustFinished => true,
            // Not present shouldn't hold up the complete check
            WaitStatus::NotPresent => true,
            _ => false,
        }
    }

    pub fn just_finished(&self) -> bool {
        match &self.wait_status {
            WaitStatus::JustFinished => true,
            _ => false,
        }
    }
}

pub struct InitStatus {
    pub arc_status: StatusInfo,
    pub dram_status: StatusInfo,
    pub eth_status: StatusInfo,
    pub cpu_status: StatusInfo,
}

impl InitStatus {
    pub fn is_waiting(&self) -> bool {
        self.arc_status.is_waiting()
            && self.dram_status.is_waiting()
            && self.eth_status.is_waiting()
            && self.cpu_status.is_waiting()
    }

    pub fn init_complete(&self) -> bool {
        self.arc_status.is_completed()
            && self.dram_status.is_completed()
            && self.eth_status.is_completed()
            && self.cpu_status.is_completed()
    }

    pub fn init_error(&self) -> bool {
        self.arc_status.is_error()
            || self.dram_status.is_error()
            || self.eth_status.is_error()
            || self.cpu_status.is_error()
    }
}

pub enum InitType {
    Arc,
    Dram,
    Eth,
    Cpu,
}

pub struct Telemetry {
    pub board_id: u64,
}

/// Defines common functionality for all chips.
/// This is a convinence interface that allows chip type agnostic code to be written.
pub trait ChipImpl: HlComms + Send + Sync + 'static {
    /// Check if the chip has been initialized
    fn is_inititalized(&self) -> Result<InitStatus, PlatformError>;

    fn update_init_state(&self, status: &mut InitStatus) -> Result<(), PlatformError>;

    /// Returns the current arch of the chip, can be used to avoid
    /// needing to ducktype when downcasting.
    fn get_arch(&self) -> Arch;

    /// Get telemetry information from the chip.
    /// The information is not cached, so should not be called repeatedly.
    fn get_telemetry(&self) -> Result<Telemetry, PlatformError>;

    /// Send an arc_msg to the underlying chip.
    fn arc_msg(&self, msg: ArcMsgOptions) -> Result<ArcMsgOk, PlatformError>;

    /// Get a list of neighbouring chips.
    /// Will return an empty list for gs and up to four chips for wh.
    fn get_neighbouring_chips(&self) -> Result<Vec<NeighbouringChip>, PlatformError>;

    /// Convinence function to downcast to a concrete type.
    fn as_any(&self) -> &dyn std::any::Any;

    /// Get information about the underlying chip transport.
    /// This is a hack to get the physical id of the chip.
    fn get_device_info(&self) -> Result<Option<DeviceInfo>, PlatformError>;
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

impl HlComms for Chip {
    fn comms_obj(&self) -> (&dyn ChipComms, &dyn ChipInterface) {
        self.inner.comms_obj()
    }
}

impl ChipImpl for Chip {
    fn is_inititalized(&self) -> Result<InitStatus, PlatformError> {
        self.inner.is_inititalized()
    }

    fn update_init_state(&self, status: &mut InitStatus) -> Result<(), PlatformError> {
        self.inner.update_init_state(status)
    }

    fn get_arch(&self) -> Arch {
        self.inner.get_arch()
    }

    fn arc_msg(&self, msg: ArcMsgOptions) -> Result<ArcMsgOk, PlatformError> {
        self.inner.arc_msg(msg)
    }

    fn get_neighbouring_chips(&self) -> Result<Vec<NeighbouringChip>, PlatformError> {
        self.inner.get_neighbouring_chips()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self.inner.as_any()
    }

    fn get_telemetry(&self) -> Result<Telemetry, PlatformError> {
        self.inner.get_telemetry()
    }

    fn get_device_info(&self) -> Result<Option<DeviceInfo>, PlatformError> {
        self.inner.get_device_info()
    }
}
