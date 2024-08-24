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
                        gs.axi_sread32(format!("ARC_RESET.SCRATCH[0]")).unwrap()
                    );
                }

                if let Some(bh) = v.as_bh() {
                    dbg!(bh.get_telemetry().unwrap());
                    dbg!(bh.get_telemetry().unwrap());
                }

                (v.get_arch(), remote, status, eth_status)
            })
        );
    }
}
