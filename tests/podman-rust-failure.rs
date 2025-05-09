/*
 * Testcase written to reproduce https://github.com/tenstorrent/tt-metal/issues/18506
 *
 * Copyright 2025, Troy Benjegerdes, using support from Google's Gemini AI model
 *
 * Licensed under the Apache v2.0 license of the Tenstorrent Luwen project
 *
 * run this on bare metal, and in a podman (or docker) rootless container
 * to demonstrate an apparent bug in the rust file_type/is_char_device librarie(s)
 *
 *
 * Run this after compiling with rustc with:
 * luwen$ rustc tests/podman-rust-failure
 * luwen$ podman run -it -v $PWD/podman-rust-failure:/f debian:testing /f
 * Is "/dev/random" a character device: true
 * char devices found: []
 */

use std::fs::File;
use std::os::unix::fs::FileTypeExt;
use std::path::Path;

fn main() -> Result<(), std::io::Error> {
    let path = Path::new("/dev/random");

    // Open the file in read-only mode.
    match File::open(path) {
        Ok(file) => {
            // Get the metadata of the file.
            match file.metadata() {
                Ok(metadata) => {
                    println!(
                        "Is {:?} a char device: {}",
                        path,
                        metadata.file_type().is_char_device()
                    );
                }
                Err(e) => eprintln!("Error getting metadata: {}", e),
            }
        }
        Err(e) => eprintln!("Error opening /dev/random: {}", e),
    }

    let output = std::fs::read_dir("/dev");
    let output = match output {
        Ok(output) => output,
        Err(err) => {
            println!("When reading /dev for a scan hit error: {err}");
            return Err(err);
        }
    };

    let mut output: Vec<_> = output
        .filter_map(|entry| {
            let entry = entry.ok()?;

            if !entry.file_type().ok()?.is_char_device() {
                return None;
            }

            entry.file_name().to_str().map(|s| s.to_string())
        })
        .collect();

    output.sort();

    println!("char devices found: {output:?}");

    Ok(())
}
