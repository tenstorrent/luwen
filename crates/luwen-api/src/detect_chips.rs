// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;

use luwen_core::Arch;

use crate::{
    chip::{wait_for_init, Chip, InitError, InitStatus},
    error::{BtWrapper, PlatformError},
    ChipImpl, EthAddr,
};

#[derive(PartialEq, Eq, Hash, Debug, Clone)]
enum InterfaceIdOrCoord {
    Id(u32),
    Coord(EthAddr),
}

/// Represents a chip object which may or may not be initialized.
pub enum UninitChip {
    /// A partially initialized chip, it may be unsafe (0xffffffff errors) to interact with this chip.
    Partially {
        /// Contains the init status
        status: Box<InitStatus>,
        /// Returned when the chip is explicitly upgraded.
        /// Or init is rerun.
        underlying: Chip,
    },
    /// The chip is fine and can be safely upgraded.
    Initialized(Chip),
}

// HACK(drosen): Probably should just implement clone on Chip...
fn clone_chip(chip: &Chip) -> Chip {
    if let Some(wh) = chip.as_wh() {
        Chip::from(Box::new(wh.clone()) as Box<dyn ChipImpl>)
    } else if let Some(gs) = chip.as_gs() {
        Chip::from(Box::new(gs.clone()) as Box<dyn ChipImpl>)
    } else if let Some(bh) = chip.as_bh() {
        Chip::from(Box::new(bh.clone()) as Box<dyn ChipImpl>)
    } else {
        unimplemented!(
            "Don't have a clone handler for chip with arch {:?}.",
            chip.get_arch()
        )
    }
}

impl Clone for UninitChip {
    fn clone(&self) -> Self {
        match self {
            Self::Partially { status, underlying } => Self::Partially {
                status: status.clone(),
                underlying: clone_chip(underlying),
            },
            Self::Initialized(chip) => Self::Initialized(clone_chip(chip)),
        }
    }
}

impl UninitChip {
    pub fn new(status: InitStatus, chip: &Chip) -> Self {
        let chip = clone_chip(chip);
        if status.init_complete() && !status.has_error() {
            UninitChip::Initialized(chip)
        } else {
            UninitChip::Partially {
                status: Box::new(status),
                underlying: chip,
            }
        }
    }

    pub fn status(&self) -> Option<&InitStatus> {
        match self {
            UninitChip::Partially { status, .. } => Some(status),
            UninitChip::Initialized(_) => None,
        }
    }

    /// Initialize the chip, if init fails at this point then we return a result
    /// instead of an UninitChip.
    pub fn init<E>(
        self,
        init_callback: &mut impl FnMut(crate::chip::ChipDetectState) -> Result<(), E>,
    ) -> Result<Chip, InitError<E>> {
        match self {
            UninitChip::Partially { mut underlying, .. } => {
                wait_for_init(&mut underlying, init_callback, false, false)?;

                Ok(underlying)
            }
            UninitChip::Initialized(chip) => Ok(chip),
        }
    }

    pub fn upgrade(self) -> Chip {
        match self {
            UninitChip::Partially { underlying, .. } => underlying,
            UninitChip::Initialized(chip) => chip,
        }
    }

    pub fn try_upgrade(&self) -> Option<&Chip> {
        match self {
            UninitChip::Partially { status, underlying } => {
                if status.init_complete() && !status.has_error() {
                    Some(underlying)
                } else {
                    None
                }
            }
            UninitChip::Initialized(chip) => Some(chip),
        }
    }

    pub fn is_initialized(&self) -> bool {
        match self {
            UninitChip::Partially { status, .. } => status.init_complete(),
            UninitChip::Initialized(_) => true,
        }
    }

    pub fn is_healthy(&self) -> Option<bool> {
        match self {
            UninitChip::Partially { status, .. } => {
                if status.init_complete() {
                    Some(status.has_error())
                } else {
                    None
                }
            }
            UninitChip::Initialized(_) => Some(true),
        }
    }

    pub fn arc_alive(&self) -> bool {
        match self {
            UninitChip::Partially { status, .. } => {
                !status.arc_status.is_waiting() && !status.arc_status.has_error()
            }
            UninitChip::Initialized(_) => true,
        }
    }

    pub fn dram_safe(&self) -> bool {
        match self {
            UninitChip::Partially { status, .. } => {
                !status.dram_status.is_waiting() && !status.dram_status.has_error()
            }
            UninitChip::Initialized(_) => true,
        }
    }

    pub fn eth_safe(&self) -> bool {
        match self {
            UninitChip::Partially { status, .. } => {
                !status.eth_status.is_waiting() && !status.eth_status.has_error()
            }
            UninitChip::Initialized(_) => true,
        }
    }

    pub fn cpu_safe(&self) -> bool {
        match self {
            UninitChip::Partially { status, .. } => {
                !status.cpu_status.is_waiting() && !status.cpu_status.has_error()
            }
            UninitChip::Initialized(_) => true,
        }
    }
}

pub struct ChipDetectOptions {
    /// If true, we will continue searching for chips even if we encounter a *recoverable* error.
    /// If false, detection errors will be raised as an Err(..).
    pub continue_on_failure: bool,
    /// If true, then we will search for chips directly available over a physical interface (pci, jtag, i2c, etc...)
    /// If false, we will search for chips directly available and via ethernet.
    pub local_only: bool,
    /// If len > 0 then only chips with the given archs will be returned.
    pub chip_filter: Vec<Arch>,
    /// If true, then we will not initialize anything that might cause a problem (i.e. a noc hang).
    pub noc_safe: bool,
}

impl Default for ChipDetectOptions {
    fn default() -> Self {
        Self {
            continue_on_failure: true,
            local_only: false,
            chip_filter: Vec::new(),
            noc_safe: false,
        }
    }
}

impl ChipDetectOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn continue_on_failure(mut self, continue_on_failure: bool) -> Self {
        self.continue_on_failure = continue_on_failure;
        self
    }

    pub fn local_only(mut self, local_only: bool) -> Self {
        self.local_only = local_only;
        self
    }

    pub fn noc_safe(mut self, noc_safe: bool) -> Self {
        self.noc_safe = noc_safe;
        self
    }
}

/// Find all chips accessible from the given set of root chips.
/// For the most part this should be a set of chips found via a PCI scan, but it doens't have to be.
///
/// The most important part of this algorithm is determining which chips are duplicates of other chips.
/// In general two boards can be differentiated by their board id, but this is not always the case.
/// For example the gs or wh X2, in that case we must fallback on the interface id for grayskull or ethernet address for wh.
/// However this does not cover all cases, if there is a wh X2 that is not in the root_chips list (which could be because it is in a neighbouring hose)
/// and both chips are in two seperate meshes with the same ethernet address. We will incorrectly detect them as being one chip.
///
/// Search steps:
/// 1. Add all given chips to output list removing duplicates this will ensure that if list indexes are used to
///    assign a chip id pci chips will always be output instead of the remote equivalent.
/// 2. To a depth first search for each root chip, adding all new chips found to the output list.
///
/// When continue on failure is true, we report errors, but continue searching for chips.
/// We pass all chips that did not complete initializations as UninitChip, the user will see the status and can
/// decide for themselves if they want to upgrade the chip to a full Chip.
/// Error Cases:
/// 1. ARC fw is hung, this usually means that there is a noc hang as well.
///    a. Not catastrophic, we can recover from the hang by resetting the chip.
/// 2. DRAM is not trained
///    a. Not catastrophic, but we should not pass this over as a good chip as we may get a noc hang when accessing DRAM.
/// 3. ARC did not complete initialization
///    a. Not catastrophic, but for gs we will have no thermal control.
/// 3. Ethernet fw is corrupted, we check this by looking for a known fw version.
///    a. Not catastrophic, we need to report this, but can continue exploring other chips in the mesh.
/// 4. Ethernet fw is hung, this usually means that the ethernet is in a bad state.
///    a. Not catastrophic, we need to report this, but can continue exploring other chips in the mesh.
/// 5. 0xffffffff error, this means that the underlying transport is hung.
///    a. This is catastrophic, we cannot continue searching for chips, because some of the chips in the mesh may no longer be accesible
///    b. We could recover from this by rerunning the search, but this is not implemented.
pub fn detect_chips<E>(
    mut root_chips: Vec<Chip>,
    init_callback: &mut impl FnMut(crate::chip::ChipDetectState) -> Result<(), E>,
    options: ChipDetectOptions,
) -> Result<Vec<UninitChip>, InitError<E>> {
    let ChipDetectOptions {
        continue_on_failure,
        local_only,
        chip_filter,
        noc_safe,
    } = options;

    let mut remotes_to_investigate = Vec::new();
    let mut seen_chips = HashSet::new();

    let mut output = Vec::new();
    for (root_index, root_chip) in root_chips.iter_mut().enumerate() {
        if !chip_filter.is_empty() && !chip_filter.contains(&root_chip.get_arch()) {
            Err(PlatformError::WrongChipArchs {
                actual: root_chip.get_arch(),
                expected: chip_filter.clone(),
                backtrace: BtWrapper::capture(),
            })?;
        }

        let status = wait_for_init(root_chip, init_callback, continue_on_failure, noc_safe)?;

        // We now want to convert to the uninitialized chip type.
        let chip = UninitChip::new(status, root_chip);

        // At this point we may not be able to talk to the chip over ethernet, there should have been an error output to the terminal,
        // so we will just not perform remote chip detection.
        let remote_ready = chip.eth_safe();
        let arc_ready = chip.arc_alive();

        output.push(chip);

        let ident = if let Some(wh) = root_chip.as_wh() {
            if arc_ready {
                if let Ok(telem) = root_chip.get_telemetry() {
                    // If WH UBB - skip ethernet exploration
                    let board_type: u64 =
                        telem.board_id_low as u64 | ((telem.board_id_high as u64) << 32);
                    let board_upi: u64 = (board_type >> 36) & 0xFFFFF;
                    const WH_6U_GLX_UPI: u64 = 0x35;

                    // Only investigate remotes if its not a UBB board or if we are not in noc_safe mode.
                    if !local_only && remote_ready && board_upi != WH_6U_GLX_UPI {
                        remotes_to_investigate.push(root_index);
                    }

                    (
                        Some(telem.board_id),
                        Some(InterfaceIdOrCoord::Coord(wh.get_local_chip_coord()?)),
                    )
                } else {
                    continue;
                }
            } else {
                continue;
            }
        } else {
            (
                // Can't fetch board id from old gs chips
                // this shouldn't matter anyway because we can only access them
                // via pci
                None,
                root_chip
                    .get_device_info()?
                    .map(|v| InterfaceIdOrCoord::Id(v.interface_id)),
            )
        };

        if !seen_chips.insert(ident) {
            continue;
        }
    }

    for root_chip in remotes_to_investigate.into_iter().map(|v| &root_chips[v]) {
        let mut to_check = root_chip.get_neighbouring_chips()?;

        let mut seen_coords = HashSet::new();
        while let Some(nchip) = to_check.pop() {
            if !nchip.routing_enabled {
                continue;
            }

            if !seen_coords.insert(nchip.eth_addr) {
                continue;
            }

            if !chip_filter.is_empty() && !chip_filter.contains(&root_chip.get_arch()) {
                continue;
            }

            if let Some(wh) = root_chip.as_wh() {
                let mut wh = wh.open_remote(nchip.eth_addr)?;

                let status = wait_for_init(&mut wh, init_callback, continue_on_failure, noc_safe)?;

                let local_coord = wh.get_local_chip_coord()?;

                if local_coord != nchip.eth_addr {
                    Err(PlatformError::Generic(
                        format!("When detecting chips in mesh found a mismatch between the expected chip coordinate {} and the actual {}", nchip.eth_addr, local_coord),
                        crate::error::BtWrapper::capture(),
                    ))?;
                }

                // If we cannot talk to the ARC then we cannot get the ident information so we
                // will just return the chip and not continue to search.
                if !status.arc_status.has_error() {
                    let telem = wh.get_telemetry()?;

                    let ident = (
                        Some(telem.board_id),
                        Some(InterfaceIdOrCoord::Coord(local_coord)),
                    );

                    if !seen_chips.insert(ident) {
                        init_callback(crate::chip::ChipDetectState {
                            chip: root_chip,
                            call: crate::chip::CallReason::NotNew,
                        })
                        .map_err(InitError::CallbackError)?;
                        continue;
                    }

                    for nchip in wh.get_neighbouring_chips()? {
                        to_check.push(nchip);
                    }
                }

                let chip = Chip::from(Box::new(wh) as Box<dyn ChipImpl>);
                output.push(UninitChip::new(status, &chip));
            } else {
                unimplemented!("Don't have a handler for non-WH chips with ethernet support yet.")
            }
        }
    }

    Ok(output)
}

pub fn detect_initialized_chips<E>(
    root_chips: Vec<Chip>,
    init_callback: &mut impl FnMut(crate::chip::ChipDetectState) -> Result<(), E>,
    options: ChipDetectOptions,
) -> Result<Vec<Chip>, InitError<E>> {
    let chips = detect_chips(root_chips, init_callback, options)?;

    let mut output = Vec::with_capacity(chips.len());
    for chip in chips {
        if chip.is_initialized() {
            output.push(chip.upgrade());
        } else {
            output.push(chip.init(&mut |_| Ok(()))?);
        }
    }

    Ok(output)
}

pub fn detect_chips_silent(
    root_chips: Vec<Chip>,
    options: ChipDetectOptions,
) -> Result<Vec<Chip>, PlatformError> {
    detect_initialized_chips::<std::convert::Infallible>(root_chips, &mut |_| Ok(()), options)
        .map_err(|v| match v {
            InitError::PlatformError(err) => err,
            InitError::CallbackError(_) => unreachable!(),
        })
}
