use cc;
use std::env;
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

/// Get the cargo target directory from OUT_DIR
fn get_cargo_target_dir() -> Option<PathBuf> {
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));
    let mut sub_path = out_dir.as_path();
    while let Some(parent) = sub_path.parent() {
        if parent.ends_with("target") {
            return Some(parent.to_path_buf());
        }
        sub_path = parent;
    }
    None
}

#[test]
fn test_header_compiles() {
    let target_dir = get_cargo_target_dir().expect("Could not find target directory");
    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let header_path = target_dir.join(&profile).join("luwen.h");

    // The header should exist since cargo builds the crate before running tests
    if !header_path.exists() {
        panic!(
            "Could not find generated luwen.h header file at {:?}. Make sure luwencpp is built.",
            header_path
        );
    }

    // Create a temporary C++ file that includes the header
    let test_cpp = r#"
#include "luwen.h"

// Test that we can use Chip* without compilation errors
void test_chip_forward_declaration() {
    luwen::Chip* chip = nullptr;
    luwen::LuwenGlue glue = {};
    chip = luwen::luwen_open(luwen::Arch::WORMHOLE, glue);
    
    if (chip) {
        luwen::chip_telemetry(chip);
        luwen::luwen_close(chip);
    }
}

int main() {
    test_chip_forward_declaration();
    return 0;
}
"#;

    // Create a temporary directory that will be automatically cleaned up
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let test_file = temp_dir.path().join("test_luwen_header.cpp");

    // Write the test file
    fs::write(&test_file, test_cpp).expect("Failed to write test file");

    // Get the directory containing the header
    let header_dir = header_path.parent().unwrap();

    // Determine target triple based on platform
    let target = if cfg!(target_os = "macos") && cfg!(target_arch = "aarch64") {
        "aarch64-apple-darwin"
    } else if cfg!(target_os = "macos") && cfg!(target_arch = "x86_64") {
        "x86_64-apple-darwin"
    } else if cfg!(target_os = "linux") && cfg!(target_arch = "x86_64") {
        "x86_64-unknown-linux-gnu"
    } else if cfg!(target_os = "linux") && cfg!(target_arch = "aarch64") {
        "aarch64-unknown-linux-gnu"
    } else if cfg!(target_os = "windows") && cfg!(target_arch = "x86_64") {
        "x86_64-pc-windows-msvc"
    } else {
        panic!("Unsupported platform");
    };

    // Use cc crate to compile the test file
    let mut build = cc::Build::new();
    build
        .cpp(true)
        .include(header_dir)
        .file(&test_file)
        .warnings(false)
        .flag("-std=c++11")
        .out_dir(temp_dir.path())
        .target(target)
        .host(target)
        .opt_level(0);

    // Try to compile - this will panic with a detailed error if compilation fails
    build.compile("test_luwen_header");
}
