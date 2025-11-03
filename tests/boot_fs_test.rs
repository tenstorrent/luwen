#![cfg(test)]

use luwen::pci::detect_chips;

/// Test reading boot fs tables from Blackhole chips
///
/// These tests verify that all standard boot FS tags can be read correctly
/// from a Blackhole chip's SPI ROM:
/// - boardcfg: board configuration information
/// - flshinfo: flash information
/// - cmfwcfg: firmware configuration
/// - origcfg: original firmware configuration
///
/// Note: These tests require physical hardware to run. By default, they are
/// annotated with #[ignore] to avoid false failures on systems without hardware.
/// To run all hardware tests:
///
///   cargo test --test boot_fs_test -- --ignored
///
/// The tests will automatically detect if compatible hardware is present;
/// if hardware is not found, the test will be skipped.
mod tests {
    use super::*;

    #[test]
    #[ignore = "Requires hardware with the ability to recover a broken SPI"]
    fn blackhole_test_boardcfg_tag() {
        // FIXME: This test assumes there are only BH chips in a system, which
        // is fragile. Also, should it iterate across all chips?
        let devices = detect_chips().unwrap();
        let bh = devices[0].as_bh().unwrap();

        let tag_read = bh.get_boot_fs_tables_spi_read("boardcfg").unwrap();
        assert!(tag_read.is_some(), "boardcfg tag should be present");

        let (_, fd) = tag_read.unwrap();
        assert_eq!(fd.image_tag_str(), "boardcfg", "Tag name should match");
        assert!(!fd.flags.invalid(), "Tag should not be marked as invalid");
    }

    #[test]
    #[ignore = "Requires hardware with the ability to recover a broken SPI"]
    fn blackhole_test_flshinfo_tag() {
        let devices = detect_chips().unwrap();
        let bh = devices[0].as_bh().unwrap();

        let tag_read = bh.get_boot_fs_tables_spi_read("flshinfo").unwrap();
        assert!(tag_read.is_some(), "flshinfo tag should be present");

        let (_, fd) = tag_read.unwrap();
        assert_eq!(fd.image_tag_str(), "flshinfo", "Tag name should match");
        assert!(!fd.flags.invalid(), "Tag should not be marked as invalid");
    }

    #[test]
    #[ignore = "Requires hardware with the ability to recover a broken SPI"]
    fn blackhole_test_cmfwcfg_tag() {
        let devices = detect_chips().unwrap();
        let bh = devices[0].as_bh().unwrap();

        let tag_read = bh.get_boot_fs_tables_spi_read("cmfwcfg").unwrap();
        assert!(tag_read.is_some(), "cmfwcfg tag should be present");

        let (_, fd) = tag_read.unwrap();
        assert_eq!(fd.image_tag_str(), "cmfwcfg", "Tag name should match");
        assert!(!fd.flags.invalid(), "Tag should not be marked as invalid");
    }

    #[test]
    #[ignore = "Requires hardware with the ability to recover a broken SPI"]
    fn blackhole_test_origcfg_tag() {
        let devices = detect_chips().unwrap();
        let bh = devices[0].as_bh().unwrap();

        let tag_read = bh.get_boot_fs_tables_spi_read("origcfg").unwrap();
        if tag_read.is_none() {
            println!("SKIPPED: No origcfg tag");
            return;
        }

        let (_, fd) = tag_read.unwrap();
        assert_eq!(fd.image_tag_str(), "origcfg", "Tag name should match");
        assert!(!fd.flags.invalid(), "Tag should not be marked as invalid");
    }
}
