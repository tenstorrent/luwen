// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

fn _workspace_dir() -> std::path::PathBuf {
    let output = std::process::Command::new(env!("CARGO"))
        .arg("locate-project")
        .arg("--workspace")
        .arg("--message-format=plain")
        .output()
        .unwrap()
        .stdout;
    let cargo_path = std::path::Path::new(std::str::from_utf8(&output).unwrap().trim());
    cargo_path.parent().unwrap().to_path_buf()
}

fn get_cargo_target_dir() -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR")?);
    let profile = std::env::var("PROFILE")?;
    let mut target_dir = None;
    let mut sub_path = out_dir.as_path();
    while let Some(parent) = sub_path.parent() {
        if parent.ends_with(&profile) {
            target_dir = Some(parent);
            break;
        }
        sub_path = parent;
    }
    let target_dir = target_dir.ok_or("not found")?;
    Ok(target_dir.to_path_buf())
}

fn main() {
    let result = cbindgen::Builder::new()
        .with_pragma_once(true)
        .with_namespace("luwen")
        .with_crate(std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .generate()
        .expect("Unable to generate bindings");

    // Cargo deb can't access the OUT_DIR variable, so we need to copy it to the directory
    // that this file will end up in.
    // let include_dir = std::env::var("OUT_DIR").unwrap();
    let include_dir = std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| {
        get_cargo_target_dir()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string()
    });
    let include_dir = std::path::Path::new(&include_dir);

    result.write_to_file(include_dir.join("luwen.h"));

    println!("cargo:rerun-if-changed=build.rs");
}
