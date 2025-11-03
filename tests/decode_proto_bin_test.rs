#![cfg(test)]

use luwen::pci::detect_chips;

/// Test utilities for verifying boot filesystem protobuf decoding
///
/// These tests verify:
/// - Boot filesystem protobuf binary decoding functionality
/// - Access to different protobuf tables (boardcfg, flshinfo, cmfwcfg, origcfg)
/// - Successful deserialization of protobuf messages
/// - Successful error return when looking for a non-existent table
///
/// Note: These tests require physical hardware to run. By default, they are
/// annotated with #[ignore] to avoid false failures on systems without hardware.
/// To run all hardware tests:
///
///   cargo test --test decode_proto_bin_test -- --ignored
///
/// The tests will automatically detect if compatible hardware is present;
/// if hardware is not found, the test will be skipped.
mod test_utils;

mod tests {
    use super::*;
    use test_utils::has_chip_type;

    #[test]
    #[ignore = "Requires hardware with the ability to recover a broken SPI"]
    fn blackhole_test_decode_boardcfg() {
        assert!(
            has_chip_type(|chip| chip.as_bh().is_some()),
            "Test requires a Blackhole chip"
        );

        let devices = detect_chips().unwrap();
        for device in devices {
            if let Some(bh) = device.as_bh() {
                // Test decoding the boardcfg table from the boot fs
                let decode_msg = bh.decode_boot_fs_table("boardcfg");
                println!("Decoded boardcfg: {decode_msg:#?}");

                // Verify the decoding was successful
                assert!(decode_msg.is_ok(), "Failed to decode boardcfg table");
                let boardcfg = decode_msg.unwrap();

                // Verify the boardcfg message contains valid data
                assert!(!boardcfg.is_empty(), "Boardcfg message should not be empty");
            }
        }
    }

    #[test]
    #[ignore = "Requires hardware with the ability to recover a broken SPI"]
    fn blackhole_test_decode_flshinfo() {
        assert!(
            has_chip_type(|chip| chip.as_bh().is_some()),
            "Test requires a Blackhole chip"
        );

        let devices = detect_chips().unwrap();
        for device in devices {
            if let Some(bh) = device.as_bh() {
                // Test decoding the flshinfo table from the boot fs
                let decode_msg = bh.decode_boot_fs_table("flshinfo");
                println!("Decoded flshinfo: {decode_msg:#?}");

                // Verify the decoding was successful
                assert!(decode_msg.is_ok(), "Failed to decode flshinfo table");
                let flshinfo = decode_msg.unwrap();

                // Verify the flshinfo message contains valid data
                assert!(!flshinfo.is_empty(), "Flshinfo message should not be empty");
            }
        }
    }

    #[test]
    #[ignore = "Requires hardware with the ability to recover a broken SPI"]
    fn blackhole_test_decode_cmfwcfg() {
        assert!(
            has_chip_type(|chip| chip.as_bh().is_some()),
            "Test requires a Blackhole chip"
        );

        let devices = detect_chips().unwrap();
        for device in devices {
            if let Some(bh) = device.as_bh() {
                // Test decoding the cmfwcfg table from the boot fs
                let decode_msg = bh.decode_boot_fs_table("cmfwcfg");
                println!("Decoded cmfwcfg: {decode_msg:#?}");

                // Verify the decoding was successful
                assert!(decode_msg.is_ok(), "Failed to decode cmfwcfg table");
                let cmfwcfg = decode_msg.unwrap();

                // Verify the cmfwcfg message contains valid data
                assert!(!cmfwcfg.is_empty(), "Cmfwcfg message should not be empty");
            }
        }
    }

    #[test]
    #[ignore = "Requires hardware with the ability to recover a broken SPI"]
    fn blackhole_test_decode_origcfg() {
        assert!(
            has_chip_type(|chip| chip.as_bh().is_some()),
            "Test requires a Blackhole chip"
        );

        let devices = detect_chips().unwrap();
        for device in devices {
            if let Some(bh) = device.as_bh() {
                // Test decoding the origcfg table from the boot fs
                let decode_msg = bh.decode_boot_fs_table("origcfg");
                println!("Decoded origcfg: {decode_msg:#?}");
                // There isn't much else to test here -- most boards
                // don't actually have an origcfg so there's nothing
                // to validate.
            }
        }
    }

    #[test]
    #[ignore = "Requires hardware with the ability to recover a broken SPI"]
    fn blackhole_test_decode_nonexistent_table() {
        assert!(
            has_chip_type(|chip| chip.as_bh().is_some()),
            "Test requires a Blackhole chip"
        );

        let devices = detect_chips().unwrap();
        for device in devices {
            if let Some(bh) = device.as_bh() {
                // Test decoding a non-existent table
                let decode_msg = bh.decode_boot_fs_table("nonexistent_table");

                // Verify the operation fails as expected
                assert!(
                    decode_msg.is_err(),
                    "Decoding non-existent table should fail"
                );
                println!(
                    "Expected error for non-existent table: {:?}",
                    decode_msg.err()
                );
            }
        }
    }
}
