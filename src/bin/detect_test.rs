use luwen_if::{chip::HlCommsInterface, ChipImpl};

fn main() {
    let partial_chips = match luwen_ref::detect_chips_fallible() {
        Ok(chips) => chips,
        Err(err) => panic!("{}", err),
    };

    for chip in partial_chips {
        let status = chip.status();
        dbg!(status);
        println!(
            "Chip: {:?}",
            chip.try_upgrade().map(|v| {
                let eth_status = chip.eth_safe();
                let remote = if let Some(wh) = v.as_wh() {
                    if chip.arc_alive() {
                        println!("{:X}", wh.get_telemetry().unwrap().board_id_low);
                    }

                    wh.is_remote
                } else {
                    false
                };

                if let Some(gs) = v.as_gs() {
                    println!(
                        "{:x}",
                        gs.axi_sread32("ARC_RESET.SCRATCH[0]").unwrap()
                    );
                }

                if let Some(bh) = v.as_bh() {
                    dbg!(bh.get_telemetry().unwrap());
                    dbg!(bh.get_telemetry().unwrap());

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

                    println!(
                        "{:x}",
                        bh.axi_sread32("arc_ss.reset_unit.SCRATCH_RAM[0]")
                            .unwrap()
                    );

                    /*
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

                    for _ in 0..1 {
                        dbg!(bh.message_queue.get_queue_info(&bh, 2).unwrap());

                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }


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

                (v.get_arch(), remote, status, eth_status)
            })
        );
    }
}
