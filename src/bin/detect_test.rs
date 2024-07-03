use luwen_if::ChipImpl;

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
                        println!("{:X}", wh.get_telemetry().unwrap().smbus_tx_board_id_low);
                    }

                    wh.is_remote
                } else {
                    false
                };

                if let Some(bh) = v.as_bh() {
                    let result = bh.arc_msg(luwen_if::chip::ArcMsgOptions {
                        msg: luwen_if::ArcMsg::Raw {
                            msg: 0x90,
                            arg0: 103,
                            arg1: 0,
                        },
                        ..Default::default()
                    });
                    dbg!(result);

                    dbg!(bh.get_telemetry());

                    let mut output = vec![0; 100];
                    dbg!(bh.spi_read(0, &mut output));
                    let mut addr = 0;
                    for o in output.chunks_exact(4) {
                        let value = u32::from_le_bytes([o[0], o[1], o[2], o[3]]);
                        println!("0x{addr:08x} 0x{value:08x}");
                        addr += 4;
                    }
                }

                (v.get_arch(), remote, status, eth_status)
            })
        );
    }
}
