#![cfg(test)]

use serial_test::serial;

use luwen::api::{chip::ArcMsgOptions, ArcMsg, ArcMsgOk, ChipImpl, TypedArcMsg};
use luwen::kmd::PciDevice;

/// Test utilities for verifying PCI device register read/write operations
///
/// These tests verify:
/// - Aligned and unaligned 32-bit register reads/writes
/// - Block memory operations with different alignment offsets
/// - Data integrity through read-after-write verification
/// - Boundary conditions for register access
///
/// Note: These tests require physical hardware to run. By default, they are
/// annotated with #[ignore] to avoid false failures on systems without hardware.
/// To run all hardware tests:
///
///   cargo test --test read_write_test -- --ignored
///
/// The tests will automatically detect if compatible hardware is present;
/// if hardware is not found, the test will be skipped.
#[serial]
mod tests {
    use luwen::api::chip::HlComms;
    use luwen::pci::detect_chips_fallible;

    use super::*;

    // Common test fixture setup
    struct TestFixture {
        raw_device: PciDevice,
        aligned_addr: u32,
    }

    impl TestFixture {
        /// Sets up a test environment with a properly aligned memory address for testing
        /// Returns None if no suitable hardware is found, which will cause tests to be skipped
        fn setup() -> Option<Self> {
            for id in PciDevice::scan() {
                let raw_device = match PciDevice::open(id) {
                    Ok(device) => device,
                    Err(_) => continue,
                };

                let device = match luwen::pci::open(id) {
                    Ok(dev) => dev,
                    Err(_) => continue,
                };

                if let Some(wh) = device.as_wh() {
                    // Get SPI dump address from chip via ARC message
                    let dump_addr = if let Ok(result) = wh.arc_msg(ArcMsgOptions {
                        msg: ArcMsg::Typed(TypedArcMsg::GetSpiDumpAddr),
                        ..Default::default()
                    }) {
                        match result {
                            ArcMsgOk::Ok { rc: _, arg } => Some(arg),
                            ArcMsgOk::OkBuf(_) => unreachable!(),
                            ArcMsgOk::OkNoWait => None,
                        }
                    } else {
                        None
                    }
                    .unwrap();

                    // Translate to physical memory space
                    let csm_offset =
                        wh.arc_if.axi_translate("ARC_CSM.DATA[0]").unwrap().addr - 0x10000000_u64;

                    // Calculate test memory location
                    let addr = csm_offset + u64::from(dump_addr);

                    // Ensure 4-byte alignment for tests
                    let aligned_addr = (addr + 3) & !3;

                    return Some(TestFixture {
                        raw_device,
                        aligned_addr: u32::try_from(aligned_addr).unwrap_or(0),
                    });
                }
            }
            None
        }
    }

    /// Helper function to reset test area to a known pattern
    fn reset_test_area(fixture: &mut TestFixture, pattern: u32) {
        fixture
            .raw_device
            .write32(fixture.aligned_addr, pattern)
            .unwrap();

        fixture
            .raw_device
            .write32(fixture.aligned_addr + 4, pattern)
            .unwrap();
    }

    #[test]
    #[cfg_attr(
        not(all(feature = "test_hardware", feature = "test_wormhole")),
        ignore = "Requires real wormhole hardware"
    )]
    fn wormhole_test_aligned_register_access() {
        let mut fixture = TestFixture::setup().expect("Hardware should be available");

        // Test 1: Basic aligned write/read of 32-bit value
        fixture
            .raw_device
            .write32(fixture.aligned_addr, 0x0000_faca)
            .unwrap();
        let readback = fixture.raw_device.read32(fixture.aligned_addr).unwrap();
        assert_eq!(
            readback, 0x0000_faca,
            "Aligned read/write of 0xfaca failed, got 0x{readback:x}"
        );

        // Test 2: Aligned write/read with 32-bit pattern
        fixture
            .raw_device
            .write32(fixture.aligned_addr, 0xcdcd_cdcd)
            .unwrap();
        let readback = fixture.raw_device.read32(fixture.aligned_addr).unwrap();
        assert_eq!(
            readback, 0xcdcd_cdcd,
            "Aligned read/write of 0xcdcdcdcd failed, got 0x{readback:x}"
        );

        // Test 3: Aligned write/read at next word boundary
        fixture
            .raw_device
            .write32(fixture.aligned_addr + 4, 0xcdcd_cdcd)
            .unwrap();
        let readback = fixture.raw_device.read32(fixture.aligned_addr + 4).unwrap();
        assert_eq!(
            readback, 0xcdcd_cdcd,
            "Aligned read/write at next word boundary failed, got 0x{readback:x}"
        );
    }

    #[test]
    #[cfg_attr(
        not(all(feature = "test_hardware", feature = "test_wormhole")),
        ignore = "Requires real wormhole hardware"
    )]
    fn wormhole_test_unaligned_register_access() {
        let mut fixture = TestFixture::setup().expect("Hardware should be available");

        // Reset test area to known pattern
        reset_test_area(&mut fixture, 0xcdcd_cdcd);

        // Test 4: Unaligned write with byte offset +1
        fixture
            .raw_device
            .write32(fixture.aligned_addr + 1, 0xdead)
            .unwrap();

        // Verify cross-boundary effects on adjacent words
        let readback = fixture.raw_device.read32(fixture.aligned_addr).unwrap();
        assert_eq!(
            readback, 0xdeadcd,
            "Unaligned write +1 effect on current word failed, got 0x{readback:x}"
        );

        let readback = fixture.raw_device.read32(fixture.aligned_addr + 4).unwrap();
        assert_eq!(
            readback, 0xcdcdcd00,
            "Unaligned write +1 effect on next word failed, got 0x{readback:x}"
        );

        // Reset test area to known pattern
        reset_test_area(&mut fixture, 0xcdcd_cdcd);

        // Test 5: Unaligned write with byte offset +3 (word boundary -1)
        fixture
            .raw_device
            .write32(fixture.aligned_addr + 3, 0xc0ffe)
            .unwrap();

        // Verify cross-boundary effects
        let readback = fixture.raw_device.read32(fixture.aligned_addr).unwrap();
        assert_eq!(
            readback, 0xfecdcdcd,
            "Unaligned write +3 effect on current word failed, got 0x{readback:x}"
        );

        let readback = fixture.raw_device.read32(fixture.aligned_addr + 4).unwrap();
        assert_eq!(
            readback, 0xcd000c0f,
            "Unaligned write +3 effect on next word failed, got 0x{readback:x}"
        );
    }

    #[test]
    #[cfg_attr(
        not(all(feature = "test_hardware", feature = "test_wormhole")),
        ignore = "Requires real wormhole hardware"
    )]
    fn wormhole_test_sequential_pattern_reads() {
        let mut fixture = TestFixture::setup().expect("Hardware should be available");

        // Test 6: Write sequential pattern for readback tests
        fixture
            .raw_device
            .write32(fixture.aligned_addr, 0x01234567)
            .unwrap();
        let readback = fixture.raw_device.read32(fixture.aligned_addr).unwrap();
        assert_eq!(
            readback, 0x01234567,
            "Sequential pattern write/read failed, got 0x{readback:x}"
        );

        // Write to adjacent word
        fixture
            .raw_device
            .write32(fixture.aligned_addr + 4, 0xabcdef)
            .unwrap();
        let readback = fixture.raw_device.read32(fixture.aligned_addr + 4).unwrap();
        assert_eq!(
            readback, 0xabcdef,
            "Sequential pattern write/read at next word failed, got 0x{readback:x}"
        );

        // Test 7: Verify unaligned reads with sequential data
        // Read with +1 byte offset (crosses word boundary)
        let readback = fixture.raw_device.read32(fixture.aligned_addr + 1).unwrap();
        assert_eq!(
            readback, 0xef012345,
            "Unaligned read +1 with sequential pattern failed, got 0x{readback:x}"
        );

        // Read with +3 byte offset (crosses word boundary)
        let readback = fixture.raw_device.read32(fixture.aligned_addr + 3).unwrap();
        assert_eq!(
            readback, 0xabcdef01,
            "Unaligned read +3 with sequential pattern failed, got 0x{readback:x}"
        );
    }

    #[test]
    #[cfg_attr(
        not(all(feature = "test_hardware", feature = "test_wormhole")),
        ignore = "Requires real wormhole hardware"
    )]
    fn wormhole_test_aligned_block_operations() {
        let mut fixture = TestFixture::setup().expect("Hardware should be available");

        // Test 8: Block write/read with aligned address
        let mut write_buffer = Vec::new();
        write_buffer.extend(0xcdcd_cdcdu32.to_le_bytes());
        write_buffer.extend(0xcdcd_cdcdu32.to_le_bytes());

        fixture
            .raw_device
            .write_block(fixture.aligned_addr, &write_buffer)
            .unwrap();

        // Verify block read matches written data
        let mut readback_buffer = vec![0u8; write_buffer.len()];
        fixture
            .raw_device
            .read_block(fixture.aligned_addr, &mut readback_buffer)
            .unwrap();

        assert_eq!(
            write_buffer, readback_buffer,
            "Aligned block write/read failed"
        );
    }

    #[test]
    #[cfg_attr(
        not(all(feature = "test_hardware", feature = "test_wormhole")),
        ignore = "Requires real wormhole hardware"
    )]
    fn wormhole_test_unaligned_block_operations() {
        let mut fixture = TestFixture::setup().expect("Hardware should be available");

        // Reset test area
        let mut reset_buffer = Vec::new();
        reset_buffer.extend(0xcdcd_cdcdu32.to_le_bytes());
        reset_buffer.extend(0xcdcd_cdcdu32.to_le_bytes());

        fixture
            .raw_device
            .write_block(fixture.aligned_addr, &reset_buffer)
            .unwrap();

        // Test 9: Unaligned block write with 2-byte data at offset +1
        let write_buffer = vec![0xad, 0xde];
        fixture
            .raw_device
            .write_block(fixture.aligned_addr + 1, &write_buffer)
            .unwrap();

        // Read back full word to verify partial write behavior
        let mut readback_buffer = vec![0u8; 4];
        fixture
            .raw_device
            .read_block(fixture.aligned_addr, &mut readback_buffer)
            .unwrap();

        assert_eq!(
            [0xcd, 0xad, 0xde, 0xcd],
            readback_buffer.as_slice(),
            "Unaligned block write with offset +1 failed"
        );

        // Reset test area
        fixture
            .raw_device
            .write_block(fixture.aligned_addr, &reset_buffer)
            .unwrap();

        // Test 10: Unaligned block write at word boundary-1 (offset +3)
        let write_buffer = vec![0xad, 0xde];
        fixture
            .raw_device
            .write_block(fixture.aligned_addr + 3, &write_buffer)
            .unwrap();

        // Read extended range to verify cross-boundary behavior
        let mut readback_buffer = vec![0u8; 7];
        fixture
            .raw_device
            .read_block(fixture.aligned_addr, &mut readback_buffer)
            .unwrap();

        assert_eq!(
            [0xcd, 0xcd, 0xcd, 0xad, 0xde, 0xcd, 0xcd],
            readback_buffer.as_slice(),
            "Unaligned block write with offset +3 failed"
        );
    }

    #[test]
    #[cfg_attr(
        not(all(feature = "test_hardware", feature = "test_wormhole")),
        ignore = "Requires real wormhole hardware"
    )]
    fn wormhole_test_sequential_pattern_block_operations() {
        let mut fixture = TestFixture::setup().expect("Hardware should be available");

        // Test 11: Block write with sequential pattern for boundary tests
        let mut write_buffer = Vec::new();
        write_buffer.extend(0x01234567u32.to_le_bytes());
        write_buffer.extend(0xabcdefu32.to_le_bytes());

        fixture
            .raw_device
            .write_block(fixture.aligned_addr, &write_buffer)
            .unwrap();

        // Verify block read with sequential pattern
        let mut readback_buffer = vec![0u8; write_buffer.len()];
        fixture
            .raw_device
            .read_block(fixture.aligned_addr, &mut readback_buffer)
            .unwrap();

        assert_eq!(
            write_buffer, readback_buffer,
            "Sequential pattern block write/read failed"
        );

        // Test 12: Verify 32-bit register reads match block reads
        let reg_readback = fixture.raw_device.read32(fixture.aligned_addr + 1).unwrap();
        assert_eq!(
            reg_readback, 0xef012345,
            "Register read at +1 doesn't match expected pattern, got 0x{reg_readback:x}"
        );

        // Verify unaligned block read at +1 offset
        let mut readback_buffer = vec![0u8; 4];
        fixture
            .raw_device
            .read_block(fixture.aligned_addr + 1, &mut readback_buffer)
            .unwrap();

        assert_eq!(
            [0x45, 0x23, 0x01, 0xef],
            readback_buffer.as_slice(),
            "Unaligned block read at +1 failed"
        );

        // Verify unaligned block read at +3 offset
        let mut readback_buffer = vec![0u8; 4];
        fixture
            .raw_device
            .read_block(fixture.aligned_addr + 3, &mut readback_buffer)
            .unwrap();

        assert_eq!(
            [0x01, 0xef, 0xcd, 0xab],
            readback_buffer.as_slice(),
            "Unaligned block read at +3 failed"
        );
    }

    #[test]
    #[cfg_attr(
        not(all(feature = "test_hardware", feature = "test_wormhole")),
        ignore = "Requires real wormhole hardware"
    )]
    #[allow(clippy::cast_possible_truncation)]
    fn wormhole_test_large_block_transfers() {
        let mut fixture = TestFixture::setup().expect("Hardware should be available");

        // Test 13: Larger block transfers (1KB) with sequential pattern
        let mut write_buffer = vec![0; 1024];
        for (index, r) in write_buffer.iter_mut().enumerate() {
            // The modulo ensures we never exceed u8 range
            let value = index % 256;
            // Safe cast: we've ensured the value is < 256, which fits in u8
            *r = value as u8;
        }

        fixture
            .raw_device
            .write_block(fixture.aligned_addr, &write_buffer)
            .unwrap();

        // Verify large block read with aligned address
        let mut readback_buffer = vec![0u8; write_buffer.len()];
        fixture
            .raw_device
            .read_block(fixture.aligned_addr, &mut readback_buffer)
            .unwrap();

        assert_eq!(
            write_buffer, readback_buffer,
            "Large (1KB) aligned block transfer failed"
        );

        // Test 14: Large block read with unaligned address (+3)
        fixture
            .raw_device
            .write_block(fixture.aligned_addr, &write_buffer)
            .unwrap();

        // Verify partial data matching with offset consideration
        let mut readback_buffer = vec![0u8; write_buffer.len()];
        fixture
            .raw_device
            .read_block(fixture.aligned_addr + 3, &mut readback_buffer)
            .unwrap();

        assert_eq!(
            write_buffer[3..],
            readback_buffer[..readback_buffer.len() - 3],
            "Large block read with offset +3 failed"
        );

        // Test 15: Large block write with unaligned address
        let mut write_buffer = vec![0; 1024];
        for (index, r) in write_buffer.iter_mut().enumerate() {
            // The modulo ensures we never exceed u8 range
            let value = index % 256;
            // Safe cast: we've ensured the value is < 256, which fits in u8
            *r = value as u8;
        }

        fixture
            .raw_device
            .write_block(fixture.aligned_addr + 1, &write_buffer)
            .unwrap();

        // Verify large block read from same unaligned address
        let mut readback_buffer = vec![0u8; write_buffer.len()];
        fixture
            .raw_device
            .read_block(fixture.aligned_addr + 1, &mut readback_buffer)
            .unwrap();

        assert_eq!(
            write_buffer, readback_buffer,
            "Large block transfer with offset +1 failed"
        );
    }

    #[test]
    #[cfg_attr(
        not(all(feature = "test_hardware", feature = "test_blackhole")),
        ignore = "Requires real blackhole hardware"
    )]
    #[allow(clippy::cast_possible_truncation)]
    fn blackhole_test_large_block_transfers_broadcast() {
        let bh = detect_chips_fallible().expect("need to be able to talk to the chips");
        let bh = bh
            .iter()
            .filter_map(|chip| chip.try_upgrade())
            .filter_map(|chip| chip.as_bh())
            .next()
            .expect("one working BH chip");

        let mut write_buffer = vec![0; 1024];
        for (index, r) in write_buffer.iter_mut().enumerate() {
            // The modulo ensures we never exceed u8 range
            let value = index % 256;
            // Safe cast: we've ensured the value is < 256, which fits in u8
            *r = value as u8;
        }

        let telem = bh
            .get_telemetry()
            .expect("need to be able to fetch telemetry");

        let mut all_tensix = Vec::new();
        for y in 2..=11 {
            for x in 1..=7 {
                all_tensix.push((x, y));
            }

            for x in 10..=16 {
                all_tensix.push((x, y));
            }
        }

        let working_cores = if telem.noc_translation_enabled {
            bh.noc_multicast(0, (2, 3), (1, 2), 0, &write_buffer)
                .expect("multicast to succeed");

            let mut working_tensix = Vec::with_capacity(all_tensix.len());

            let working_cols = telem.tensix_enabled_col.count_ones();
            for core in all_tensix {
                let x = core.0 as u32;
                if (x <= 7 && x < working_cols) || (x >= 10 && (x - 2) < working_cols) {
                    working_tensix.push(core);
                }
            }

            working_tensix
        } else {
            bh.noc_broadcast(0, 0, &write_buffer)
                .expect("broadcast to succeed");

            let mut working_col_bitmask = telem.tensix_enabled_col;

            let mut working_cols = Vec::new();
            let mut col = 0;
            let tensix_cols = [1, 2, 3, 4, 5, 6, 7, 10, 11, 12, 13, 14, 15, 16];
            while working_col_bitmask != 0 {
                if working_col_bitmask & 0x1 != 0 {
                    working_cols.push(tensix_cols[col]);
                }
                working_col_bitmask >>= 1;
                col += 1;
            }

            let mut working_tensix = Vec::with_capacity(all_tensix.len());
            for core in all_tensix {
                if working_cols.contains(&core.0) {
                    working_tensix.push(core);
                }
            }

            working_tensix
        };

        for core in working_cores {
            println!("Checking core {core:?}");

            let mut readback_buffer = vec![0u8; write_buffer.len()];
            bh.noc_read(0, core.0, core.1, 0, &mut readback_buffer)
                .expect("readback to succeed");

            assert_eq!(
                write_buffer, readback_buffer,
                "Write to core {core:?} failed"
            );
        }
    }
}
