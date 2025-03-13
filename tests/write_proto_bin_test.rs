#![cfg(test)]
use luwen_ref::detect_chips;

/// Test utilities for verifying boot filesystem protobuf updating
///
/// These tests verify:
/// - Successfuly update cmfwcfg - asic_fmax to 1000 and aiclk_ppm_en to false
/// - Update the flashinfo table with a new value
///
/// Note: These tests require physical hardware to run. By default, they are
/// annotated with #[ignore] to avoid false failures on systems without hardware.
/// To run all hardware tests:
///
///   cargo test --test write_proto_bin_test -- --ignored
///
/// The tests will automatically detect if compatible hardware is present;
/// if hardware is not found, the test will be skipped.
mod test_utils;
use serde_json::json;

mod tests {
    use super::*;
    use test_utils::has_chip_type;

    #[test]
    #[ignore = "Requires hardware"]
    fn blackhole_test_write_cmfwcfg() {
        assert!(
            has_chip_type(|chip| chip.as_bh().is_some()),
            "Test requires a Blackhole chip"
        );

        let devices = detect_chips().unwrap();
        for device in devices {
            if let Some(bh) = device.as_bh() {
                // Test decoding the cmfwcfg table from the boot fs
                let decode_msg = bh.decode_boot_fs_table("cmfwcfg");

                // Verify the decoding was successful
                assert!(decode_msg.is_ok(), "Failed to decode cmfwcfg table");
                let mut cmfwcfg = decode_msg.unwrap();

                // Update "asic_fmax" to 1000
                if let Some(feature_enable) = cmfwcfg.get_mut("chip_limits") {
                    if let Some(asic_fmax) = feature_enable.get_mut("asic_fmax") {
                        *asic_fmax = json!(1000);  // Modify the value anc convert to a serde_json Value
                    }
                }
                // Update "aiclk_ppm_en" to false
                if let Some(feature_enable) = cmfwcfg.get_mut("feature_enable") {
                    if let Some(aiclk_ppm_en) = feature_enable.get_mut("aiclk_ppm_en") {
                        *aiclk_ppm_en = json!(false);  // Modify the value anc convert to a serde_json Value
                    }
                }

                // convert hashmap to fw_table obj
                let cmfwcfg_obj = spirom_tables::from_hash_map::<spirom_tables::fw_table::FwTable>(cmfwcfg.clone());
                println!("Updated cmfwcfg object: {:#?}", cmfwcfg_obj);

                // Verify the cmfwcfg message contains valid data
                assert!(!cmfwcfg.is_empty(), "Cmfwcfg message should not be empty");
                bh.encode_and_write_boot_fs_table(cmfwcfg_obj, "cmfwcfg").unwrap();
                println!("Successfully wrote cmfwcfg to device");
            }
        }
    }

    #[test]
    #[ignore = "Requires hardware"]
    fn blackhole_test_write_flshinfo() {
        assert!(
            has_chip_type(|chip| chip.as_bh().is_some()),
            "Test requires a Blackhole chip"
        );
        let devices = detect_chips().unwrap();
        for device in devices {
            if let Some(bh) = device.as_bh() {
                // Test decoding the flshinfo table from the boot fs
                let decode_msg = bh.decode_boot_fs_table("flshinfo");

                // Verify the decoding was successful
                assert!(decode_msg.is_ok(), "Failed to decode flshinfo table");
                let mut flshinfo = decode_msg.unwrap();
                println!("Decoded flshinfo: {:#?}", flshinfo);

                if let Some(date_programmed) = flshinfo.get_mut("date_programmed") {
                        *date_programmed = json!(111111);  // Modify the value anc convert to a serde_json Value
                }
                // convert hashmap to fw_table obj
                let flshinfo_obj = spirom_tables::from_hash_map::<spirom_tables::flash_info::FlashInfoTable>(flshinfo.clone());
                println!("Updated flshinfo object: {:#?}", flshinfo_obj);

                // Verify the flshinfo message contains valid data
                assert!(!flshinfo.is_empty(), "flshinfo message should not be empty");
                bh.encode_and_write_boot_fs_table(flshinfo_obj, "flshinfo").unwrap();
                println!("Successfully wrote flshinfo to device");
            }
        }
    }

}
