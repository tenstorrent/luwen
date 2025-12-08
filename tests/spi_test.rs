#![cfg(test)]

use luwen::pci::detect_chips;

/// Test SPI read/write operations on chips
///
/// These tests verify that SPI flash memory can be properly read and written.
///
/// The tests perform:
/// - Reading board information from a fixed SPI address
/// - Incrementing a counter in a spare/scratch area of SPI
/// - Reading back the incremented value to verify write operation
///
/// Note: These tests require physical hardware to run. By default, they are
/// annotated with #[ignore] to avoid false failures on systems without hardware.
/// To run all hardware tests:
///
///   cargo test --test spi_test -- --ignored
///
/// The tests will automatically detect if compatible hardware is present;
/// if hardware is not found, the test will be skipped.
mod test_utils;

mod tests {
    use super::*;

    #[test]
    #[cfg_attr(
        not(all(feature = "test_hardware", feature = "test_blackhole")),
        ignore = "Requires hardware with the ability to recover a broken SPI"
    )]
    fn blackhole_test_spi_operations() {
        let devices = detect_chips().unwrap();

        // Board info address is common for all devices
        let board_info_addr = 0x20108;

        for device in devices {
            if let Some(bh) = device.as_bh() {
                // Test reading board information
                let mut board_info = [0u8; 8];
                bh.spi_read(board_info_addr, &mut board_info).unwrap();
                let board_info_value = u64::from_le_bytes(board_info);
                println!("Blackhole BOARD_INFO: {board_info_value:016X}");
                assert_ne!(board_info_value, 0, "Board info should not be zero");

                // Test read-modify-write on spare/scratch area
                let spare_addr = 0x20134; // Blackhole spare area

                // Read current value
                let mut original_value = [0u8; 2];
                bh.spi_read(spare_addr, &mut original_value).unwrap();
                println!(
                    "Original value at 0x{:X}: {:02X}{:02X}",
                    spare_addr, original_value[1], original_value[0]
                );

                // Increment value (create a change)
                let mut new_value = original_value;
                new_value[0] = new_value[0].wrapping_add(1);
                if new_value[0] == 0 {
                    new_value[1] = new_value[1].wrapping_add(1);
                }

                // Write back incremented value
                bh.spi_write(spare_addr, &new_value).unwrap();

                // Read back to verify
                let mut verify_value = [0u8; 2];
                bh.spi_read(spare_addr, &mut verify_value).unwrap();
                println!(
                    "Updated value at 0x{:X}: {:02X}{:02X}",
                    spare_addr, verify_value[1], verify_value[0]
                );

                // Verify read-after-write
                assert_eq!(
                    verify_value, new_value,
                    "SPI write verification failed: expected {new_value:?}, got {verify_value:?}"
                );

                // Read wider area to check SPI handling of different sizes
                let mut wide_value = [0u8; 8];
                bh.spi_read(spare_addr, &mut wide_value).unwrap();
                let wide_value_u64 = u64::from_le_bytes(wide_value);
                println!("Wide read at 0x{spare_addr:X}: {wide_value_u64:016X}");

                // Verify first 2 bytes match our written value
                assert_eq!(
                    wide_value[0], new_value[0],
                    "First byte of wide read doesn't match written value"
                );
                assert_eq!(
                    wide_value[1], new_value[1],
                    "Second byte of wide read doesn't match written value"
                );
            }
        }
    }

    #[test]
    #[cfg_attr(
        not(all(feature = "test_hardware", feature = "test_wormhole")),
        ignore = "Requires hardware with the ability to recover a broken SPI"
    )]
    fn wormhole_test_spi_operations() {
        let devices = detect_chips().unwrap();

        // Board info address is common for all devices
        let board_info_addr = 0x20108;

        for device in devices {
            if let Some(wh) = device.as_wh() {
                // Test reading board information
                let mut board_info = [0u8; 8];
                wh.spi_read(board_info_addr, &mut board_info).unwrap();
                let board_info_value = u64::from_le_bytes(board_info);
                println!("Wormhole BOARD_INFO: {board_info_value:016X}");
                assert_ne!(board_info_value, 0, "Board info should not be zero");

                // Test read-modify-write on spare/scratch area
                let spare_addr = 0x20134; // Wormhole spare area

                // Read current value
                let mut original_value = [0u8; 2];
                wh.spi_read(spare_addr, &mut original_value).unwrap();
                println!(
                    "Original value at 0x{:X}: {:02X}{:02X}",
                    spare_addr, original_value[1], original_value[0]
                );

                // Increment value (create a change)
                let mut new_value = original_value;
                new_value[0] = new_value[0].wrapping_add(1);
                if new_value[0] == 0 {
                    new_value[1] = new_value[1].wrapping_add(1);
                }

                // Write back incremented value
                wh.spi_write(spare_addr, &new_value).unwrap();

                // Read back to verify
                let mut verify_value = [0u8; 2];
                wh.spi_read(spare_addr, &mut verify_value).unwrap();
                println!(
                    "Updated value at 0x{:X}: {:02X}{:02X}",
                    spare_addr, verify_value[1], verify_value[0]
                );

                // Verify read-after-write
                assert_eq!(
                    verify_value, new_value,
                    "SPI write verification failed: expected {new_value:?}, got {verify_value:?}"
                );

                // Read wider area to check SPI handling of different sizes
                let mut wide_value = [0u8; 8];
                wh.spi_read(spare_addr, &mut wide_value).unwrap();
                let wide_value_u64 = u64::from_le_bytes(wide_value);
                println!("Wide read at 0x{spare_addr:X}: {wide_value_u64:016X}");

                // Verify first 2 bytes match our written value
                assert_eq!(
                    wide_value[0], new_value[0],
                    "First byte of wide read doesn't match written value"
                );
                assert_eq!(
                    wide_value[1], new_value[1],
                    "Second byte of wide read doesn't match written value"
                );
            }
        }
    }
}
