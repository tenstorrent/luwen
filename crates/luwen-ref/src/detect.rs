// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{convert::Infallible, io::IsTerminal};

use indicatif::ProgressBar;
use luwen_if::{
    chip::{
        Chip, ChipDetectState, CommsStatus, ComponentStatusInfo, HlCommsInterface, InitError,
        InitStatus,
    },
    error::PlatformError,
    CallbackStorage, ChipDetectOptions, ChipImpl, UninitChip,
};
use ttkmd_if::PciDevice;

use crate::{comms_callback, error::LuwenError, ExtendedPciDevice};

pub struct InProgressDetect {
    /// Options passed into the start of the detect function
    pub options: ChipDetectOptions,
    /// Information about any chips that failed the initial probe
    pub failed_chips: Vec<(usize, Chip, PlatformError)>,
    /// Chips that have had the initial probe succeed but have not yet been checked for completed init
    pub chips: Vec<Chip>,
}

pub fn start_detect(options: ChipDetectOptions) -> Result<InProgressDetect, LuwenError> {
    let mut chips = Vec::new();
    let mut failed_chips = Vec::new();

    let device_ids = PciDevice::scan();
    for device_id in device_ids {
        let ud = ExtendedPciDevice::open(device_id)?;

        let arch = ud.borrow().device.arch;

        let chip = Chip::open(arch, CallbackStorage::new(comms_callback, ud.clone()))?;

        // First let's test basic pcie communication we may be in a hang state so it's
        // important that we let the detect function know

        // Hack(drosen): Basic init procedure should resolve this
        let scratch_0 = if chip.get_arch().is_blackhole() {
            "arc_ss.reset_unit.SCRATCH_0"
        } else {
            "ARC_RESET.SCRATCH[0]"
        };
        let result = chip.axi_sread32(scratch_0);
        if let Err(err) = result {
            // Basic comms have failed... we should output a nice error message on the console
            failed_chips.push((device_id, chip, err));
        } else {
            chips.push(chip);
        }
    }

    Ok(InProgressDetect {
        options,
        failed_chips,
        chips,
    })
}

impl InProgressDetect {
    pub fn detect<E>(
        self,
        mut callback: impl FnMut(ChipDetectState) -> Result<(), E>,
    ) -> Result<Vec<UninitChip>, InitError<E>> {
        let mut chips = luwen_if::detect_chips(self.chips, &mut callback, self.options);

        if let Ok(chips) = &mut chips {
            for (id, chip, err) in self.failed_chips.into_iter() {
                let mut status = InitStatus::new_unknown();
                status.comms_status = CommsStatus::CommunicationError(err.to_string());
                status.unknown_state = false;
                chips.insert(
                    id,
                    UninitChip::Partially {
                        status: Box::new(status),
                        underlying: chip,
                    },
                );
            }
        }

        chips
    }
}

pub fn detect_chips_options_notui(
    options: ChipDetectOptions,
) -> Result<Vec<UninitChip>, LuwenError> {
    let detect = start_detect(options)?;

    // First we will output errors for the chips we already know have failed
    for (id, _, err) in &detect.failed_chips {
        eprintln!("Failed to communicate over pcie with chip {id}: {err}");
    }

    let mut total_chips = detect.failed_chips.len();

    let mut last_output = String::new();

    let init_callback = |status: ChipDetectState| {
        match status.call {
            luwen_if::chip::CallReason::NotNew => {
                total_chips = total_chips.saturating_sub(1);
                eprintln!("Ok I was wrong we actually have {total_chips} chips");
            }
            luwen_if::chip::CallReason::NewChip => {
                total_chips += 1;
                eprintln!("New chip! We now have {total_chips} chips");
            }
            luwen_if::chip::CallReason::InitWait(status) => {
                let mut output = String::new();
                output.push_str(&status.arc_status.to_string());
                output.push('\n');
                output.push_str(&status.dram_status.to_string());
                output.push('\n');
                output.push_str(&status.eth_status.to_string());
                output.push('\n');
                output.push_str(&status.cpu_status.to_string());

                if last_output != output {
                    eprintln!("Initializing chip...\n{output}");
                    last_output = output;
                }
            }
            luwen_if::chip::CallReason::ChipInitCompleted(status) => {
                eprintln!("Chip initialization complete (found {last_output})");

                let mut output = String::new();
                output.push_str(&status.arc_status.to_string());
                output.push('\n');
                output.push_str(&status.dram_status.to_string());
                output.push('\n');
                output.push_str(&status.eth_status.to_string());
                output.push('\n');
                output.push_str(&status.cpu_status.to_string());

                if status.has_error() {
                    eprintln!("Chip initializing failed...\n{output}");
                } else {
                    eprintln!("Chip initializing complete...\n{output}");
                }
            }
        };

        Ok::<(), Infallible>(())
    };

    let chips = match detect.detect(init_callback) {
        Err(InitError::CallbackError(err)) => {
            eprintln!("Ran into error from status callback;\n{err}");
            return Err(luwen_if::error::PlatformError::Generic(
                "Hit error from status callback".to_string(),
                luwen_if::error::BtWrapper::capture(),
            ))?;
        }
        Err(InitError::PlatformError(err)) => {
            return Err(err)?;
        }

        Ok(chips) => chips,
    };

    eprintln!("Chip detection complete (found {last_output})");

    Ok(chips)
}

pub fn detect_chips_options_tui(options: ChipDetectOptions) -> Result<Vec<UninitChip>, LuwenError> {
    let chip_detect_bar = indicatif::ProgressBar::new_spinner().with_style(
        indicatif::ProgressStyle::default_spinner()
            .template("{spinner:.green} Detecting chips (found {pos})")
            .unwrap(),
    );

    let mut chip_init_bar = None;
    let mut arc_init_bar = None;
    let mut dram_init_bar = None;
    let mut eth_init_bar = None;
    let mut cpu_init_bar = None;

    fn add_bar(bars: &indicatif::MultiProgress) -> ProgressBar {
        let new_bar = bars.add(
            indicatif::ProgressBar::new_spinner().with_style(
                indicatif::ProgressStyle::default_spinner()
                    .template("{spinner:.green} {msg}")
                    .unwrap(),
            ),
        );
        new_bar.set_message("Initializing Chip");
        new_bar.enable_steady_tick(std::time::Duration::from_secs_f32(1.0 / 30.0));

        new_bar
    }

    fn update_bar_with_status<P: std::fmt::Display, E: std::fmt::Display>(
        bars: &indicatif::MultiProgress,
        bar: &mut Option<ProgressBar>,
        status: &ComponentStatusInfo<P, E>,
    ) {
        if bar.is_none() && status.is_present() {
            *bar = Some(add_bar(bars));
        }

        if let Some(bar) = bar {
            if status.is_waiting() && status.is_present() {
                bar.set_message(status.to_string());
            }
        }
    }

    fn maybe_remove_bar<P, E>(
        bars: &indicatif::MultiProgress,
        bar: &mut Option<ProgressBar>,
        status: &ComponentStatusInfo<P, E>,
    ) {
        if let Some(bar) = bar.take() {
            if status.has_error() {
                bar.finish();
            } else {
                bar.finish_and_clear();
                bars.remove(&bar);
            }
        }
    }

    let detect = start_detect(options)?;

    let bars = indicatif::MultiProgress::new();
    let chip_detect_bar = bars.add(chip_detect_bar);
    chip_detect_bar.enable_steady_tick(std::time::Duration::from_secs_f32(1.0 / 30.0));

    // First we will output errors for the chips we already know have failed
    for (id, _, err) in &detect.failed_chips {
        chip_detect_bar.inc(1);
        let bar = add_bar(&bars);
        bar.finish_with_message(format!(
            "Failed to communicate over pcie with chip {id}: {err}"
        ));
    }

    let init_callback = |status: ChipDetectState| {
        match status.call {
            luwen_if::chip::CallReason::NotNew => {
                chip_detect_bar.set_position(chip_detect_bar.position().saturating_sub(1));
            }
            luwen_if::chip::CallReason::NewChip => {
                chip_detect_bar.inc(1);
                chip_init_bar = Some(add_bar(&bars));
            }
            luwen_if::chip::CallReason::InitWait(status) => {
                update_bar_with_status(&bars, &mut arc_init_bar, &status.arc_status);
                update_bar_with_status(&bars, &mut dram_init_bar, &status.dram_status);
                update_bar_with_status(&bars, &mut eth_init_bar, &status.eth_status);
                update_bar_with_status(&bars, &mut cpu_init_bar, &status.cpu_status);

                if let Some(bar) = chip_init_bar.as_ref() {
                    bar.set_message("Waiting chip to initialize".to_string());
                }
            }
            luwen_if::chip::CallReason::ChipInitCompleted(status) => {
                chip_detect_bar.set_message("Chip initialization complete (found {pos})");

                maybe_remove_bar(&bars, &mut arc_init_bar, &status.arc_status);
                maybe_remove_bar(&bars, &mut dram_init_bar, &status.dram_status);
                maybe_remove_bar(&bars, &mut eth_init_bar, &status.eth_status);
                maybe_remove_bar(&bars, &mut cpu_init_bar, &status.cpu_status);

                if let Some(bar) = chip_init_bar.take() {
                    if status.has_error() {
                        bar.finish_with_message("Chip initialization failed");
                    } else {
                        bar.finish_and_clear();
                        bars.remove(&bar);
                    }
                }
            }
        };

        Ok::<(), Infallible>(())
    };

    let chips = match detect.detect(init_callback) {
        Err(InitError::CallbackError(err)) => {
            chip_detect_bar
                .finish_with_message(format!("Ran into error from status callback;\n{err}"));
            return Err(luwen_if::error::PlatformError::Generic(
                "Hit error from status callback".to_string(),
                luwen_if::error::BtWrapper::capture(),
            ))?;
        }
        Err(InitError::PlatformError(err)) => {
            return Err(err)?;
        }

        Ok(chips) => chips,
    };

    chip_detect_bar.finish_with_message("Chip detection complete (found {pos})");

    // The tui will end with the cursor on the same line as our bar, so let's go to a new line
    eprintln!();

    Ok(chips)
}

pub fn detect_chips_silent(options: ChipDetectOptions) -> Result<Vec<UninitChip>, LuwenError> {
    match start_detect(options)?.detect(|_state| Ok::<(), Infallible>(())) {
        Ok(chips) => Ok(chips),
        Err(InitError::CallbackError(_)) => unreachable!("Somehow got an infallible error"),
        Err(InitError::PlatformError(err)) => Err(err.into()),
    }
}

pub fn detect_chips_options(options: ChipDetectOptions) -> Result<Vec<UninitChip>, LuwenError> {
    if std::io::stderr().is_terminal() {
        detect_chips_options_tui(options)
    } else {
        detect_chips_options_notui(options)
    }
}

pub fn detect_chips_fallible() -> Result<Vec<UninitChip>, LuwenError> {
    detect_chips_options(ChipDetectOptions::default())
}

pub fn detect_chips() -> Result<Vec<Chip>, LuwenError> {
    let chips = detect_chips_fallible()?;

    let mut output = Vec::with_capacity(chips.len());
    for chip in chips {
        output.push(
            chip.init(&mut |_| Ok::<(), Infallible>(()))
                .map_err(Into::<luwen_if::error::PlatformError>::into)?,
        );
    }

    Ok(output)
}

pub fn detect_local_chips() -> Result<Vec<Chip>, LuwenError> {
    let chips = detect_chips_options(ChipDetectOptions {
        local_only: true,
        ..Default::default()
    })?;

    let mut output = Vec::with_capacity(chips.len());
    for chip in chips {
        output.push(
            chip.init(&mut |_| Ok::<(), Infallible>(()))
                .map_err(Into::<luwen_if::error::PlatformError>::into)?,
        );
    }

    Ok(output)
}
