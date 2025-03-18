use luwen_if::{chip::HlCommsInterface, ChipImpl};

/// A diagnostic utility for detecting and querying chip status.
///
/// This tool:
/// - Detects all supported chips in the system
/// - Reports initialization status of each chip (ARC, ETH, DRAM)
/// - Retrieves chip-specific information (telemetry, board IDs)
/// - Tests basic chip functionality (ARC messaging, register access)
///
/// Used primarily for hardware verification and debugging chip communication.
fn main() {
    // Use fallible detection to get detailed error information when chips fail to initialize
    let partial_chips = match luwen_ref::detect_chips_fallible() {
        Ok(chips) => chips,
        Err(err) => panic!("{}", err),
    };

    for chip in partial_chips {
        // Capture status for diagnostic reporting
        let status = chip.status();
        dbg!(status);
        println!(
            "Chip: {:?}",
            // Upgrade to access architecture-specific functionality
            chip.try_upgrade().map(|v| {
                // ETH safety check required before certain operations
                let eth_status = chip.eth_safe();

                // For Wormhole chips, show board ID and check if remote
                let remote = if let Some(wh) = v.as_wh() {
                    if chip.arc_alive() {
                        // Board ID helps identify specific hardware
                        println!("{:X}", wh.get_telemetry().unwrap().board_id_low);
                    }

                    wh.is_remote
                } else {
                    false
                };

                // Check Grayskull-specific registers
                if let Some(gs) = v.as_gs() {
                    println!(
                        "{:x}",
                        gs.axi_sread32("ARC_RESET.SCRATCH[0]").unwrap()
                    );
                }

                // Blackhole-specific diagnostics
                if let Some(bh) = v.as_bh() {
                    // Get telemetry twice to verify consistent readings
                    dbg!(bh.get_telemetry().unwrap());
                    dbg!(bh.get_telemetry().unwrap());

                    // Test ARC messaging with debug command (ID 0x90)
                    // Command 106 is a diagnostic message
                    let result = bh
                        .arc_msg(luwen_if::chip::ArcMsgOptions {
                            msg: luwen_if::ArcMsg::Raw {
                                msg: 0x90,
                                arg0: 106,
                                arg1: 0,
                            },
                            ..Default::default()
                        })
                        .unwrap();
                    dbg!(result);

                    // Check scratch register for ARC boot status
                    println!(
                        "{:x}",
                        bh.axi_sread32("arc_ss.reset_unit.SCRATCH_RAM[0]")
                            .unwrap()
                    );

                    /*
                    // SPI flash memory diagnostics - temporarily disabled
                    // This section tests SPI read/write functionality by:
                    // 1. Reading 100 32-bit words from SPI flash
                    // 2. Checking message queue health
                    // 3. Writing pattern data to SPI flash (8K of incremental values)
                    // 4. Reading back the written data to verify integrity

                    // Read initial flash content
                    let mut output = vec![0u32; 100];
                    {
                        let output = output.as_mut_ptr() as *mut u8;

                        let mut output =
                            unsafe { core::slice::from_raw_parts_mut(output, 100 * 4) };
                        bh.spi_read(0, &mut output).unwrap();
                    }

                    let mut addr = 0;
                    for o in output {
                        println!("0x{addr:08x} 0x{o:08x}");
                        addr += 4;
                    }

                    // Check message queue health
                    for _ in 0..1 {
                        dbg!(bh.message_queue.get_queue_info(&bh, 2).unwrap());
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }

                    // Write test pattern
                    {
                        let mut output = vec![0u32; 8 * 1024];
                        for (index, o) in output.iter_mut().enumerate() {
                            *o = index as u32;
                        }

                        {
                            let output_ptr = output.as_ptr() as *const u8;

                            let output = unsafe {
                                core::slice::from_raw_parts(output_ptr, output.len() * 4)
                            };
                            bh.spi_write(0, &output).unwrap();
                        }
                    }

                    // Verify written data
                    let mut output = vec![0u32; 8 * 1024];
                    {
                        let output_ptr = output.as_mut_ptr() as *mut u8;

                        let mut output = unsafe {
                            core::slice::from_raw_parts_mut(output_ptr, output.len() * 4)
                        };
                        bh.spi_read(0, &mut output).unwrap();
                    }

                    let mut addr = 0;
                    for o in output {
                        println!("{:x} 0x{addr:08x} 0x{o:08x}", addr / 4);
                        addr += 4;
                    }
                    */
                }

                // Return key diagnostic information in a tuple
                (v.get_arch(), remote, status, eth_status)
            })
        );
    }
}
