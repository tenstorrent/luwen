// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use kmdif::PciDevice;
use luwen_if::{
    chip::{Chip, WaitStatus},
    CallbackStorage, ChipDetectOptions, UninitChip,
};

use crate::{comms_callback, error::LuwenError, ExtendedPciDevice};

pub fn detect_chips() -> Result<Vec<UninitChip>, LuwenError> {
    let mut chips = Vec::new();

    let device_ids = PciDevice::scan();
    for device_id in device_ids {
        let ud = ExtendedPciDevice::open(device_id)?;

        let arch = ud.borrow().device.arch;

        chips.push(Chip::open(
            arch,
            CallbackStorage::new(comms_callback, ud.clone()),
        ));
    }

    let chip_detect_bar = indicatif::ProgressBar::new_spinner().with_style(
        indicatif::ProgressStyle::default_spinner()
            .template("{spinner:.green} Detecting chips (found {pos})")
            .unwrap(),
    );
    let mut chip_init_bar = None;

    let bars = indicatif::MultiProgress::new();
    let chip_detect_bar = bars.add(chip_detect_bar);
    chip_detect_bar.enable_steady_tick(std::time::Duration::from_secs_f32(1.0 / 30.0));
    let mut init_callback = |status: luwen_if::chip::ChipDetectState<'_, &dyn std::fmt::Display, &dyn std::fmt::Display>| {
        match status.call {
            luwen_if::chip::CallReason::NewChip => {
                chip_detect_bar.inc(1);
                let new_bar = bars.add(
                    indicatif::ProgressBar::new_spinner().with_style(
                        indicatif::ProgressStyle::default_spinner()
                            .template("{spinner:.green} {msg}")
                            .unwrap(),
                    ),
                );
                new_bar.set_message("Initializing Chip");
                new_bar.enable_steady_tick(std::time::Duration::from_secs_f32(1.0 / 30.0));
                chip_init_bar = Some(new_bar);
            }
            luwen_if::chip::CallReason::InitWait(component, status ) => {
                if let Some(bar) = chip_init_bar.as_ref() {
                    let mut format_message = format!("Waiting for {} to initialize", component);
                    if status.total > 1 {
                        format_message =
                            format!("{} [{}/{}]", format_message, status.ready, status.total);
                    }

                    if !status.status.is_empty() {
                        format_message = format!("{}: {}", format_message, status.status);
                    }

                    if let WaitStatus::Waiting { start, timeout } = &status.wait_status {
                        format_message =
                            format!("({}/{}) {format_message}", start.elapsed().as_secs(), timeout.as_secs());
                    }

                    bar.set_message(format_message);
                }
            }
            luwen_if::chip::CallReason::ChipInitCompleted(status) => {
                chip_detect_bar.set_message("Chip initialization complete (found {pos})");

                if let Some(bar) = chip_init_bar.take() {
                    if status.init_error() {
                        bar.finish_with_message("Chip initialization failed");
                    } else {
                        bar.finish();
                    }

                    bars.remove(&bar);
                }
            }
        };
    };

    let options = ChipDetectOptions::default();
    Ok(luwen_if::detect_chips(chips, &mut init_callback, options)?)
}

pub fn detect_initialized_chips() -> Result<Vec<Chip>, LuwenError> {
    let chips = detect_chips()?;

    let mut output = Vec::with_capacity(chips.len());
    for chip in chips {
        output.push(chip.init(&mut |_| {})?);
    }

    Ok(output)
}
