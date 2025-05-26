use cc;
use std::env;
use std::fs;
use std::path::PathBuf;

/// Get the cargo target directory in a robust way
fn get_cargo_target_dir() -> PathBuf {
    // First try CARGO_TARGET_DIR environment variable
    if let Ok(target_dir) = env::var("CARGO_TARGET_DIR") {
        return PathBuf::from(target_dir);
    }

    // Then try to find it relative to the manifest directory
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");

    // Go up to workspace root and then into target
    let mut path = PathBuf::from(manifest_dir);
    while !path.join("Cargo.lock").exists() && path.parent().is_some() {
        path = path.parent().unwrap().to_path_buf();
    }
    path.join("target")
}

#[test]
fn test_header_compiles() {
    let target_dir = get_cargo_target_dir();
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
    ::Chip* chip = nullptr;
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

    // Use temp directory for test files
    let temp_dir = env::temp_dir();
    let test_file = temp_dir.join("test_luwen_header.cpp");

    // Write the test file
    fs::write(&test_file, test_cpp).expect("Failed to write test file");

    // Get the directory containing the header
    let header_dir = header_path.parent().unwrap();

    // cc crate needs these environment variables (normally set by cargo in build scripts)
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

    env::set_var("TARGET", target);
    env::set_var("OPT_LEVEL", "0");
    env::set_var("HOST", target);

    // Use cc crate to compile the test file
    let mut build = cc::Build::new();
    build
        .cpp(true)
        .include(header_dir)
        .file(&test_file)
        .warnings(false)
        .flag("-std=c++11");

    // Try to compile - this will panic with a detailed error if compilation fails
    build.compile("test_luwen_header");

    // Clean up test file
    let _ = fs::remove_file(&test_file);
}
