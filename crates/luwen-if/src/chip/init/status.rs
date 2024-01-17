// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{convert::Infallible, fmt};

use thiserror::Error;

use crate::error::ArcReadyError;

#[derive(Clone, Debug)]
pub enum EthernetInitError {
    FwCorrupted,
    NotTrained,
}

impl fmt::Display for EthernetInitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EthernetInitError::FwCorrupted => f.write_str("Ethernet firmware is corrupted"),
            EthernetInitError::NotTrained => f.write_str("Ethernet is not trained"),
        }
    }
}

#[derive(Clone, Debug)]
pub enum EthernetPartialInitError {
    FwOverwritten,
}

impl fmt::Display for EthernetPartialInitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EthernetPartialInitError::FwOverwritten => {
                f.write_str("Ethernet firmware version has an invalid format and is assumed to have been overwritten")
            }
        }
    }
}

#[derive(Clone, Debug)]
pub enum ArcInitError {
    FwCorrupted,
    WaitingForInit(ArcReadyError),
    Hung,
}

impl fmt::Display for ArcInitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArcInitError::FwCorrupted => f.write_str("ARC firmware is corrupted"),
            ArcInitError::WaitingForInit(err) => write!(f, "ARC is waiting for initialization; {err}"),
            ArcInitError::Hung => f.write_str("ARC is hung"),
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub enum DramChannelStatus {
    TrainingNone,
    TrainingFail,
    TrainingPass,
    TrainingSkip,
    PhyOff,
    ReadEye,
    BistEye,
    CaDebug,
}

impl TryFrom<u8> for DramChannelStatus {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, ()> {
        match value {
            0 => Ok(DramChannelStatus::TrainingNone),
            1 => Ok(DramChannelStatus::TrainingFail),
            2 => Ok(DramChannelStatus::TrainingPass),
            3 => Ok(DramChannelStatus::TrainingSkip),
            4 => Ok(DramChannelStatus::PhyOff),
            5 => Ok(DramChannelStatus::ReadEye),
            6 => Ok(DramChannelStatus::BistEye),
            7 => Ok(DramChannelStatus::CaDebug),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Debug)]
pub enum DramInitError {
    NotTrained(DramChannelStatus),
}

impl fmt::Display for DramInitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DramInitError::NotTrained(_) => f.write_str("DRAM was not able to train"),
        }
    }
}

#[derive(Clone, Debug, Error)]
pub enum CpuInitError {
    // NOTE: Mockup for BH prep
}

/// The final initialization status for a component within a chip.
/// This status is not intended to drive the initialization state machine
/// instead it gives a single high level view of the current status of a single component.
/// The NotInitialized and InitError types have their own specializations to so the caller only has
/// to match against the component type if absolutly necessary.
#[derive(Debug, Clone)]
pub enum WaitStatus<P, E> {
    NotPresent,
    Waiting,

    JustFinished,

    Done,
    /// This is used in the case where the user has specific that we shouldn't check to see if the
    /// compnent has actually been intialized.
    /// See noc_safe for an example of this enumeration being used.
    NoCheck,

    Timeout(std::time::Duration),
    NotInitialized(P),
    Error(E),
}

impl<P, E> WaitStatus<P, E> {
    pub fn is_done(&self) -> bool {
        match self {
            WaitStatus::Done => true,
            _ => false,
        }
    }
}

/// A generic structure which contains the status information for each component.
/// There is enough information here to determine the
#[derive(Debug, Clone)]
pub struct ComponentStatusInfo<P, E> {
    pub wait_status: Box<[WaitStatus<P, E>]>,
    pub timeout: std::time::Duration,
    pub start_time: std::time::Instant,
    pub status: String,
}

impl<P: fmt::Display, E: fmt::Display> fmt::Display for ComponentStatusInfo<P, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut waiting_count = 0;
        let mut completed_count = 0;
        for status in self.wait_status.iter() {
            if let WaitStatus::Waiting { .. } = status {
                waiting_count += 1;
            } else if let WaitStatus::NoCheck | WaitStatus::Done | WaitStatus::NotPresent = status {
                completed_count += 1;
            }
        }

        let completed_init = waiting_count == 0;

        let message = if !completed_init {
            format!(
                "({}/{})",
                self.start_time.elapsed().as_secs(),
                self.timeout.as_secs()
            )
        } else {
            String::new()
        };

        let message = if self.wait_status.len() > 1 {
            format!("{message} [{}/{}]", completed_count, self.wait_status.len())
        } else {
            message
        };

        let message = if !self.status.is_empty() {
            format!("{message} {}", self.status)
        } else {
            message
        };

        let mut detailed_messages = String::new();
        for status in self.wait_status.iter() {
            if let WaitStatus::Error(e) = status {
                detailed_messages = format!("{detailed_messages}\n\t{e}");
            } else if let WaitStatus::NotInitialized(e) = status {
                detailed_messages = format!("{detailed_messages}\n\t{e}");
            }
        }

        let message = format!("{message}{detailed_messages}");

        f.write_str(message.as_str())
    }
}

impl<P, E> ComponentStatusInfo<P, E> {
    pub fn not_present() -> Self {
        Self {
            wait_status: Box::new([]),
            status: "No components present".to_string(),
            timeout: std::time::Duration::default(),
            start_time: std::time::Instant::now(),
        }
    }

    pub fn init_waiting(status: String, timeout: std::time::Duration, count: usize) -> Self {
        let wait_status = (0..count).map(|_| WaitStatus::Waiting).collect();
        Self {
            wait_status,
            status,

            start_time: std::time::Instant::now(),
            timeout,
        }
    }

    pub fn is_waiting(&self) -> bool {
        for status in self.wait_status.iter() {
            match status {
                WaitStatus::Waiting => {
                    return true;
                }

                WaitStatus::NotPresent
                | WaitStatus::JustFinished
                | WaitStatus::Done
                | WaitStatus::NotInitialized(_)
                | WaitStatus::NoCheck
                | WaitStatus::Timeout(_)
                | WaitStatus::Error(_) => {}
            }
        }

        return false;
    }

    pub fn is_present(&self) -> bool {
        for status in self.wait_status.iter() {
            match status {
                WaitStatus::Waiting
                | WaitStatus::JustFinished
                | WaitStatus::Done
                | WaitStatus::NotInitialized(_)
                | WaitStatus::NoCheck
                | WaitStatus::Timeout(_)
                | WaitStatus::Error(_) => {
                    return true;
                }

                WaitStatus::NotPresent => {}
            }
        }

        return false;
    }

    pub fn has_error(&self) -> bool {
        for status in self.wait_status.iter() {
            match status {
                WaitStatus::Error(_) | WaitStatus::Timeout(_) => {
                    return true;
                }

                WaitStatus::NotPresent
                | WaitStatus::NoCheck
                | WaitStatus::JustFinished
                | WaitStatus::Done
                | WaitStatus::Waiting { .. }
                | WaitStatus::NotInitialized(_) => {}
            }
        }

        return false;
    }
}

#[derive(Clone, Debug, Default)]
pub struct InitOptions {
    /// If false, then we will not try to initialize anything that would require talking on the NOC
    pub noc_safe: bool,
}

#[derive(Clone, Debug)]
pub enum CommsStatus {
    CanCommunicate,
    CommunicationError(String),
}

impl CommsStatus {
    pub fn ok(&self) -> bool {
        match self {
            CommsStatus::CanCommunicate => true,
            CommsStatus::CommunicationError(_) => false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct InitStatus {
    pub comms_status: CommsStatus,
    pub dram_status: ComponentStatusInfo<Infallible, DramInitError>,
    pub cpu_status: ComponentStatusInfo<Infallible, CpuInitError>,
    pub arc_status: ComponentStatusInfo<Infallible, ArcInitError>,
    pub eth_status: ComponentStatusInfo<EthernetPartialInitError, EthernetInitError>,

    pub init_options: InitOptions,

    /// We cannot communicate with the chip prior to the initialization process. Therefore we start
    /// with the chip in an unknown state (all status is marked as not present).
    pub unknown_state: bool,
}

impl InitStatus {
    pub fn new_unknown() -> Self {
        InitStatus {
            comms_status: CommsStatus::CommunicationError("Haven't checked".to_string()),
            dram_status: ComponentStatusInfo::not_present(),
            cpu_status: ComponentStatusInfo::not_present(),
            arc_status: ComponentStatusInfo::not_present(),
            eth_status: ComponentStatusInfo::not_present(),
            init_options: InitOptions::default(),
            unknown_state: true,
        }
    }

    pub fn can_communicate(&self) -> bool {
        self.comms_status.ok()
    }

    pub fn is_waiting(&self) -> bool {
        self.arc_status.is_waiting()
            && self.dram_status.is_waiting()
            && self.eth_status.is_waiting()
            && self.cpu_status.is_waiting()
    }

    pub fn init_complete(&self) -> bool {
        !self.is_waiting()
    }

    pub fn has_error(&self) -> bool {
        !self.comms_status.ok()
            || self.arc_status.has_error()
            || self.dram_status.has_error()
            || self.eth_status.has_error()
            || self.cpu_status.has_error()
    }
}
