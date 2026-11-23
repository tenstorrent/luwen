// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

// Functions related to wh_ubb systems
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};
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

fn find_binary_in_paths(binary_name: &str, search_dirs: &[PathBuf]) -> Option<PathBuf> {
    for dir in search_dirs {
        let candidate = dir.join(binary_name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

pub fn check_ipmitool_installed() -> Result<PathBuf, String> {
    let mut search_dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).collect())
        .unwrap_or_default();
    for fallback in ["/usr/bin", "/usr/sbin", "/bin", "/sbin", "/usr/local/bin"] {
        let path = PathBuf::from(fallback);
        if !search_dirs.contains(&path) {
            search_dirs.push(path);
        }
    }

    find_binary_in_paths("ipmitool", &search_dirs).ok_or_else(|| {
        "ipmitool was not found on this host. Try installing it with `sudo apt install ipmitool`"
            .to_string()
    })
}

// There is no reliable native ipmi support, so issuing it as a command instead
pub fn wh_ubb_ipmi_reset(
    ubb_num: &str,
    dev_num: &str,
    op_mode: &str,
    reset_time: &str,
) -> Result<(), String> {
    if let Err(err) = check_ipmitool_installed() {
        eprintln!("{err}");
        return Err(err);
    }

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
        if Path::new(file).exists() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_binary_in_paths_locates_existing_file() {
        let temp_dir = std::env::temp_dir().join(format!("luwen_ipmitool_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();
        let fake_bin = temp_dir.join("ipmitool");
        std::fs::write(&fake_bin, b"#!/bin/sh\n").unwrap();

        assert_eq!(
            find_binary_in_paths("ipmitool", std::slice::from_ref(&temp_dir)),
            Some(fake_bin)
        );
        assert_eq!(find_binary_in_paths("nonexistent_ipmitool_bin", &[temp_dir.clone()]), None);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
