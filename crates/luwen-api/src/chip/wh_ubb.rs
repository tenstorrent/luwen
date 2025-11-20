// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

// Functions related to wh_ubb systems
use std::os::unix::fs::FileTypeExt;
use std::process::Command;
/*
COMMAND: ipmitool raw 0x30 0x8B <ubb_num> <dev_num> <op_mode> <reset_time>

@param
 ubb_num(UBB):   0x0~0xF (bit map)
 dev_num(ASIC):  0x0~0xFF(bit map)
 op_mode:        0x0 - Asserted/Deassert reset with a reset period (reset_time)
                0x1 - Asserted reset
                0x2 - Deasserted reset
 reset_time: resolution 10ms (ex. 15 => 150ms)
*/

// There is no reliable native ipmi support, so issuing it as a command instead
pub fn wh_ubb_ipmi_reset(
    ubb_num: &str,
    dev_num: &str,
    op_mode: &str,
    reset_time: &str,
) -> Result<(), String> {
    let full_command =
        format!("sudo ipmitool raw 0x30 0x8B {ubb_num} {dev_num} {op_mode} {reset_time}");
    println!("Executing command: {full_command}");

    let output = Command::new("sudo")
        .arg("ipmitool")
        .arg("raw")
        .arg("0x30")
        .arg("0x8B")
        .arg(ubb_num)
        .arg(dev_num)
        .arg(op_mode)
        .arg(reset_time)
        .output()
        .map_err(|e| format!("Failed to execute ipmitool: {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "IPMI command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

// Wait for the driver to reload, try 100 times
pub fn ubb_wait_for_driver_load() {
    let file = "/dev/tenstorrent";
    let mut attempts = 0;

    while attempts < 100 {
        let mut dev_num = 0;
        if std::path::Path::new(file).exists() {
            if let Ok(entries) = std::fs::read_dir(file) {
                for entry in entries.flatten() {
                    let Ok(ft) = entry.file_type() else { continue };
                    if ft.is_char_device() {
                        dev_num += 1;
                    }
                }
                if dev_num == 32 {
                    println!("All 32 chips found in \"/dev/tenstorrent\"");
                    return;
                }
            }
        }
        println!("Waiting for all 32 chips to show up in \"/dev/tenstorrent\", found {dev_num} chip(s) ... {attempts} seconds");
        std::thread::sleep(std::time::Duration::from_secs(1));
        attempts += 1;
    }
    // If we reach here, the driver was not loaded
    panic!("Didn't find all 32 chips in /dev/tenstorrent after 100 seconds... giving up");
}
