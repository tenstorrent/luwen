#![cfg(test)]

/// Test C++ header compilation
///
/// This test verifies that the generated luwen.h header file can be successfully
/// compiled with a C++ compiler. It creates a temporary C++ source file that
/// includes the header and uses various declarations from it.
///
/// The test performs:
/// - Locating the generated luwen.h header file
/// - Creating a temporary C++ test file that includes and uses the header
/// - Compiling the test file with a C++ compiler
/// - Verifying compilation succeeds without errors
///
/// Note: This test requires a C++ compiler to be available on the system.
/// It will attempt to use 'c++' command which should be available on most
/// Unix-like systems with a C++ compiler installed.

mod tests {
    use std::fs;
    use std::path::Path;
    use std::process::Command;

    #[test]
    fn test_header_compiles() {
        // Find the generated header file
        let header_path = if Path::new("target/debug/luwen.h").exists() {
            "target/debug/luwen.h"
        } else if Path::new("target/release/luwen.h").exists() {
            "target/release/luwen.h"
        } else {
            // Try to build it first
            Command::new("cargo")
                .args(&["build", "-p", "luwencpp"])
                .output()
                .expect("Failed to build luwencpp");

            if Path::new("target/debug/luwen.h").exists() {
                "target/debug/luwen.h"
            } else {
                panic!("Could not find generated luwen.h header file");
            }
        };

        // Create a temporary C++ file that includes the header
        let test_cpp = r#"
#include "luwen.h"

// Test that we can use Chip* without compilation errors
void test_chip_forward_declaration() {
    Chip* chip = nullptr;
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

        // Write the test file
        let test_file = "test_header_compilation.cpp";
        fs::write(test_file, test_cpp).expect("Failed to write test file");

        // Get the directory containing the header
        let header_dir = Path::new(header_path).parent().unwrap();

        // Try to compile the test file
        let output = Command::new("c++")
            .args(&[
                "-std=c++11",
                "-c",
                test_file,
                &format!("-I{}", header_dir.display()),
                "-o",
                "test_header_compilation.o",
            ])
            .output()
            .expect("Failed to execute c++ compiler");

        // Clean up
        let _ = fs::remove_file(test_file);
        let _ = fs::remove_file("test_header_compilation.o");

        // Check if compilation succeeded
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            panic!("Header compilation failed:\n{}", stderr);
        }
    }
}