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
    NoAccess,
    WaitingForInit(ArcReadyError),
    FwVersionTooOld { version: Option<u32>, required: u32 },
    Hung,
}

impl fmt::Display for ArcInitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArcInitError::NoAccess => f.write_str("Could not access ARC"),
            ArcInitError::FwCorrupted => f.write_str("ARC firmware is corrupted"),
            ArcInitError::WaitingForInit(err) => {
                write!(f, "ARC is waiting for initialization; {err}")
            }
            ArcInitError::FwVersionTooOld { version, required } => {
                let version = if let Some(version) = version {
                    format!("{version:x}")
                } else {
                    "<unknown version>".to_string()
                };
                write!(
                    f,
                    "ARC FW is older than the minimum supported version; {version} < {required:x}"
                )
            }
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

impl std::fmt::Display for DramChannelStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DramChannelStatus::TrainingNone => f.write_str("in pre-training"),
            DramChannelStatus::TrainingFail => f.write_str("failed to train"),
            DramChannelStatus::TrainingPass => f.write_str("passed training"),
            DramChannelStatus::TrainingSkip => f.write_str("skipped training"),
            DramChannelStatus::PhyOff => f.write_str("phy is off"),
            DramChannelStatus::ReadEye => f.write_str("read eye"),
            DramChannelStatus::BistEye => f.write_str("bist eye"),
            DramChannelStatus::CaDebug => f.write_str("ca debug"),
        }
    }
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
    Waiting(Option<String>),

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
        matches!(self, WaitStatus::Done)
    }
}

/// A generic structure which contains the status information for each component.
/// There is enough information here to determine the
#[derive(Debug, Clone)]
pub struct ComponentStatusInfo<P, E> {
    pub wait_status: Box<[WaitStatus<P, E>]>,
    pub timeout: std::time::Duration,
    pub start_time: std::time::Instant,
    pub name: String,
}

impl<P: fmt::Display, E: fmt::Display> fmt::Display for ComponentStatusInfo<P, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut waiting_count = 0;
        let mut completed_count = 0;
        for status in self.wait_status.iter() {
            if let WaitStatus::Waiting { .. } = status {
                waiting_count += 1;
            } else if let WaitStatus::NoCheck
            | WaitStatus::JustFinished
            | WaitStatus::Done
            | WaitStatus::NotPresent = status
            {
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
            format!("{message} [{}/{}]", completed_count, self.wait_status.len(),)
        } else {
            message
        };

        let message = format!("{message} {}", self.name);

        let mut message_options: Vec<(Vec<_>, String)> = Vec::with_capacity(self.wait_status.len());
        let mut force_oneline = true;
        for (index, status) in self.wait_status.iter().enumerate() {
            if let WaitStatus::Waiting(Some(status)) = status {
                if let Some(value) = message_options.iter_mut().find(|(_, v)| v == status) {
                    value.0.push(index);
                } else {
                    message_options.push((vec![index], status.clone()));
                }
            } else if let WaitStatus::Error(e) = status {
                let e = e.to_string();
                if let Some(value) = message_options.iter_mut().find(|v| v.1 == e) {
                    value.0.push(index);
                } else {
                    message_options.push((vec![index], e));
                }
            } else if let WaitStatus::NotInitialized(e) = status {
                let e = e.to_string();
                if let Some(value) = message_options.iter_mut().find(|v| v.1 == e) {
                    value.0.push(index);
                } else {
                    message_options.push((vec![index], e));
                }
            } else {
                force_oneline = false;
            }
        }

        let message = if message_options.len() == 1 && force_oneline {
            format!("{message}: {}", message_options[0].1)
        } else {
            let mut message = format!("{message}\n");
            for (indexes, option) in message_options {
                message = format!("\t{message}[");
                for index in indexes[..indexes.len().saturating_sub(1)].iter() {
                    message = format!("{message}{index};");
                }
                if let Some(index) = indexes.last() {
                    message = format!("{message}{index}");
                }
                message = format!("{message}]: {option}\n");
            }

            message
        };

        f.write_str(message.as_str())
    }
}

impl<P, E> ComponentStatusInfo<P, E> {
    pub fn not_present(name: String) -> Self {
        Self {
            name,
            wait_status: Box::new([]),
            timeout: std::time::Duration::default(),
            start_time: std::time::Instant::now(),
        }
    }

    pub fn init_waiting(name: String, timeout: std::time::Duration, count: usize) -> Self {
        let wait_status = (0..count).map(|_| WaitStatus::Waiting(None)).collect();
        Self {
            name,
            wait_status,

            start_time: std::time::Instant::now(),
            timeout,
        }
    }

    pub fn is_waiting(&self) -> bool {
        for status in self.wait_status.iter() {
            match status {
                WaitStatus::Waiting(_) => {
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

        false
    }

    pub fn is_present(&self) -> bool {
        for status in self.wait_status.iter() {
            match status {
                WaitStatus::Waiting(_)
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

        false
    }

    pub fn has_error(&self) -> bool {
        for status in self.wait_status.iter() {
            match status {
                WaitStatus::Error(_) | WaitStatus::Timeout(_) | WaitStatus::NoCheck => {
                    return true;
                }

                WaitStatus::NotPresent
                | WaitStatus::JustFinished
                | WaitStatus::Done
                | WaitStatus::Waiting { .. }
                | WaitStatus::NotInitialized(_) => {}
            }
        }

        false
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

impl fmt::Display for CommsStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommsStatus::CanCommunicate => f.write_str("Success"),
            CommsStatus::CommunicationError(_err) => f.write_str("Error"),
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

impl fmt::Display for InitStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fn write_component_status<P, E>(status: &ComponentStatusInfo<P, E>) -> String {
            let mut init_status = String::new();
            if status.start_time.elapsed() > status.timeout {
                init_status.push_str("Timeout");
            } else {
                init_status.push_str("In Progress");
            }
            let mut completed_count = 0;
            for status in status.wait_status.iter() {
                if let WaitStatus::NoCheck
                | WaitStatus::JustFinished
                | WaitStatus::Done
                | WaitStatus::NotPresent = status
                {
                    completed_count += 1;
                }
            }
            if !status.wait_status.is_empty() {
                init_status.push_str(
                    format!(
                        ", {} out of {} initialized",
                        completed_count,
                        status.wait_status.len()
                    )
                    .as_str(),
                );
            }
            init_status
        }
        writeln!(f, "   Communication Status: {}", self.comms_status)?;
        writeln!(
            f,
            "   DRAM Status: {}",
            write_component_status(&self.dram_status)
        )?;
        writeln!(
            f,
            "   CPU Status: {}",
            write_component_status(&self.cpu_status)
        )?;
        writeln!(
            f,
            "   ARC Status: {}",
            write_component_status(&self.arc_status)
        )?;
        writeln!(
            f,
            "   Ethernet Status: {}",
            write_component_status(&self.eth_status)
        )?;
        writeln!(f, "   Noc Safe: {:?}", self.init_options.noc_safe)?;
        writeln!(f, "   Unknown State: {}", self.unknown_state)
    }
}

impl InitStatus {
    pub fn new_unknown() -> Self {
        InitStatus {
            comms_status: CommsStatus::CommunicationError("Haven't checked".to_string()),
            dram_status: ComponentStatusInfo::not_present("DRAM".to_string()),
            cpu_status: ComponentStatusInfo::not_present("CPU".to_string()),
            arc_status: ComponentStatusInfo::not_present("ARC".to_string()),
            eth_status: ComponentStatusInfo::not_present("ETH".to_string()),
            init_options: InitOptions::default(),
            unknown_state: true,
        }
    }

    pub fn can_communicate(&self) -> bool {
        self.comms_status.ok()
    }

    pub fn is_waiting(&self) -> bool {
        self.arc_status.is_waiting()
            || self.dram_status.is_waiting()
            || self.eth_status.is_waiting()
            || self.cpu_status.is_waiting()
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
