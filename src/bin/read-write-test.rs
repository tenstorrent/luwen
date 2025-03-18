use luwen_if::{chip::ArcMsgOptions, ArcMsg, ArcMsgOk, ChipImpl, TypedArcMsg};
use ttkmd_if::PciDevice;

/// A test utility for PCI device register read/write operations.
///
/// This tool:
/// - Performs aligned and unaligned 32-bit register reads/writes
/// - Tests block memory operations with various alignment offsets
/// - Verifies data integrity through read-after-write verification
/// - Tests boundary conditions for register access
///
/// Used primarily for testing PCI device driver implementation and memory access.
fn main() {
    for id in PciDevice::scan() {
        let mut raw_device = PciDevice::open(id).unwrap();
        let device = luwen_ref::open(id).unwrap();

        if let Some(wh) = device.as_wh() {
            // Request SPI dump address from chip via ARC message
            let dump_addr = if let Ok(result) = wh.arc_msg(ArcMsgOptions {
                msg: ArcMsg::Typed(TypedArcMsg::GetSpiDumpAddr),
                ..Default::default()
            }) {
                match result {
                    ArcMsgOk::Ok { rc: _, arg } => Some(arg),
                    ArcMsgOk::OkNoWait => None,
                }
            } else {
                None
            }
            .unwrap();

            // Translate CSM register address to physical memory space
            let csm_offset =
                wh.arc_if.axi_translate("ARC_CSM.DATA[0]").unwrap().addr - 0x10000000_u64;

            // Calculate test memory location from chip-provided address
            let addr = csm_offset + (dump_addr as u64);

            // Ensure 4-byte alignment for initial tests
            let aligned_addr = (addr + 3) & !3;

            // Test 1: Basic aligned write/read of 16-bit value
            raw_device.write32(aligned_addr as u32, 0xfaca).unwrap();
            let readback = raw_device.read32(aligned_addr as u32).unwrap();
            assert_eq!(readback, 0xfaca, "{:x} != faca", readback);

            // Test 2: Aligned write/read with 32-bit pattern
            raw_device
                .write32(aligned_addr as u32, 0xcdcd_cdcd)
                .unwrap();
            let readback = raw_device.read32(aligned_addr as u32).unwrap();
            assert_eq!(readback, 0xcdcd_cdcd, "{:x} != cdcdcdcd", readback);

            // Test 3: Aligned write/read at next word boundary
            raw_device
                .write32(aligned_addr as u32 + 4, 0xcdcd_cdcd)
                .unwrap();
            let readback = raw_device.read32(aligned_addr as u32 + 4).unwrap();
            assert_eq!(readback, 0xcdcd_cdcd, "{:x} != cdcdcdcd", readback);

            // Test 4: Unaligned write with byte offset +1
            raw_device.write32(aligned_addr as u32 + 1, 0xdead).unwrap();
            // Verify cross-boundary effects on adjacent words
            let readback = raw_device.read32(aligned_addr as u32).unwrap();
            assert_eq!(readback, 0xdeadcd, "{:x} != deadcd", readback);
            let readback = raw_device.read32(aligned_addr as u32 + 4).unwrap();
            assert_eq!(readback, 0xcdcdcd00, "{:x} != 00cdcdcd", readback);

            // Reset test data and verify
            raw_device
                .write32(aligned_addr as u32, 0xcdcd_cdcd)
                .unwrap();
            let readback = raw_device.read32(aligned_addr as u32).unwrap();
            assert_eq!(readback, 0xcdcd_cdcd, "{:x} != cdcdcdcd", readback);

            // Reset adjacent word and verify
            raw_device
                .write32(aligned_addr as u32 + 4, 0xcdcd_cdcd)
                .unwrap();
            let readback = raw_device.read32(aligned_addr as u32 + 4).unwrap();
            assert_eq!(readback, 0xcdcd_cdcd, "{:x} != cdcdcdcd", readback);

            // Test 5: Unaligned write with byte offset +3 (word boundary -1)
            raw_device
                .write32(aligned_addr as u32 + 3, 0xc0ffe)
                .unwrap();
            // Verify cross-boundary effects
            let readback = raw_device.read32(aligned_addr as u32).unwrap();
            assert_eq!(readback, 0xfecdcdcd, "{:x} != fecdcdcd", readback);
            let readback = raw_device.read32(aligned_addr as u32 + 4).unwrap();
            assert_eq!(readback, 0xcd000c0f, "{:x} != c0f", readback);

            // Test 6: Write sequential pattern for readback tests
            raw_device.write32(aligned_addr as u32, 0x01234567).unwrap();
            let readback = raw_device.read32(aligned_addr as u32).unwrap();
            assert_eq!(readback, 0x01234567, "{:x} != 01234567", readback);

            // Write to adjacent word
            raw_device
                .write32(aligned_addr as u32 + 4, 0xabcdef)
                .unwrap();
            let readback = raw_device.read32(aligned_addr as u32 + 4).unwrap();
            assert_eq!(readback, 0xabcdef, "{:x} != abcdef", readback);

            // Test 7: Verify unaligned reads with sequential data
            // Read with +1 byte offset (crosses word boundary)
            let readback = raw_device.read32(aligned_addr as u32 + 1).unwrap();
            assert_eq!(readback, 0xef012345, "{:x} != ef012345", readback);

            // Read with +3 byte offset (crosses word boundary)
            let readback = raw_device.read32(aligned_addr as u32 + 3).unwrap();
            assert_eq!(readback, 0xabcdef01, "{:x} != abcdef01", readback);

            // Test 8: Block write/read with aligned address
            let mut write_buffer = Vec::new();
            write_buffer.extend(0xcdcd_cdcdu32.to_le_bytes());
            write_buffer.extend(0xcdcd_cdcdu32.to_le_bytes());
            raw_device
                .write_block(aligned_addr as u32, &write_buffer)
                .unwrap();

            // Verify block read matches written data
            let mut readback_buffer = vec![0u8; write_buffer.len()];
            raw_device
                .read_block(aligned_addr as u32, &mut readback_buffer)
                .unwrap();
            assert_eq!(write_buffer, readback_buffer);

            // Test 9: Unaligned block write with 2-byte data
            let write_buffer = vec![0xad, 0xde];
            raw_device
                .write_block(aligned_addr as u32 + 1, &write_buffer)
                .unwrap();

            // Read back full word to verify partial write behavior
            let mut readback_buffer = vec![0u8; 4];
            raw_device
                .read_block(aligned_addr as u32, &mut readback_buffer)
                .unwrap();
            assert_eq!([0xcd, 0xad, 0xde, 0xcd], readback_buffer.as_slice());

            // Reset test area with known pattern
            let mut write_buffer = Vec::new();
            write_buffer.extend(0xcdcd_cdcdu32.to_le_bytes());
            write_buffer.extend(0xcdcd_cdcdu32.to_le_bytes());
            raw_device
                .write_block(aligned_addr as u32, &write_buffer)
                .unwrap();

            // Verify reset operation
            let mut readback_buffer = vec![0u8; write_buffer.len()];
            raw_device
                .read_block(aligned_addr as u32, &mut readback_buffer)
                .unwrap();
            assert_eq!(write_buffer, readback_buffer);

            // Test 10: Unaligned block write at word boundary-1
            let write_buffer = vec![0xad, 0xde];
            raw_device
                .write_block(aligned_addr as u32 + 3, &write_buffer)
                .unwrap();

            // Read extended range to verify cross-boundary behavior
            let mut readback_buffer = vec![0u8; 7];
            raw_device
                .read_block(aligned_addr as u32, &mut readback_buffer)
                .unwrap();
            assert_eq!(
                [0xcd, 0xcd, 0xcd, 0xad, 0xde, 0xcd, 0xcd],
                readback_buffer.as_slice()
            );

            // Test 11: Block write with sequential pattern for boundary tests
            let mut write_buffer = Vec::new();
            write_buffer.extend(0x01234567u32.to_le_bytes());
            write_buffer.extend(0xabcdefu32.to_le_bytes());
            raw_device
                .write_block(aligned_addr as u32, &write_buffer)
                .unwrap();

            // Verify block read with sequential pattern
            let mut readback_buffer = vec![0u8; write_buffer.len()];
            raw_device
                .read_block(aligned_addr as u32, &mut readback_buffer)
                .unwrap();
            assert_eq!(write_buffer, readback_buffer);

            // Test 12: Verify 32-bit register reads match block reads
            let readback = raw_device.read32(aligned_addr as u32 + 1).unwrap();
            assert_eq!(readback, 0xef012345, "{:x} != ef012345", readback);

            // Verify unaligned block read at +1 offset
            let mut readback_buffer = vec![0u8; 4];
            raw_device
                .read_block(aligned_addr as u32 + 1, &mut readback_buffer)
                .unwrap();
            assert_eq!([0x45, 0x23, 0x01, 0xef], readback_buffer.as_slice());

            // Verify unaligned block read at +3 offset
            let mut readback_buffer = vec![0u8; 4];
            raw_device
                .read_block(aligned_addr as u32 + 3, &mut readback_buffer)
                .unwrap();
            assert_eq!([0x01, 0xef, 0xcd, 0xab], readback_buffer.as_slice());

            // Test 13: Larger block transfers (1KB) with sequential pattern
            let mut write_buffer = vec![0; 1024];
            for (index, r) in write_buffer.iter_mut().enumerate() {
                *r = index as u8;
            }
            raw_device
                .write_block(aligned_addr as u32, &write_buffer)
                .unwrap();

            // Verify large block read with aligned address
            let mut readback_buffer = vec![0u8; write_buffer.len()];
            raw_device
                .read_block(aligned_addr as u32, &mut readback_buffer)
                .unwrap();
            assert_eq!(write_buffer, readback_buffer);

            // Test 14: Large block read with unaligned address (+3)
            let mut write_buffer = vec![0; 1024];
            for (index, r) in write_buffer.iter_mut().enumerate() {
                *r = index as u8;
            }
            raw_device
                .write_block(aligned_addr as u32, &write_buffer)
                .unwrap();

            // Verify partial data matching with offset consideration
            let mut readback_buffer = vec![0u8; write_buffer.len()];
            raw_device
                .read_block(aligned_addr as u32 + 3, &mut readback_buffer)
                .unwrap();
            assert_eq!(
                write_buffer[3..],
                readback_buffer[..readback_buffer.len() - 3]
            );

            // Test 15: Large block write with unaligned address
            let mut write_buffer = vec![0; 1024];
            for (index, r) in write_buffer.iter_mut().enumerate() {
                *r = index as u8;
            }
            raw_device
                .write_block(aligned_addr as u32 + 1, &write_buffer)
                .unwrap();

            // Verify large block read from same unaligned address
            let mut readback_buffer = vec![0u8; write_buffer.len()];
            raw_device
                .read_block(aligned_addr as u32 + 1, &mut readback_buffer)
                .unwrap();
            assert_eq!(write_buffer, readback_buffer);
        }
    }
}
