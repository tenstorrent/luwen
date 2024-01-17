use luwen_if::ChipImpl;

fn main() {
    let partial_chips = match luwen_ref::detect_chips_fallible() {
        Ok(chips) => chips,
        Err(err) => panic!("{}", err),
    };

    for chip in partial_chips {
        let status = chip.status();
        println!(
            "Chip: {:?}",
            chip.try_upgrade().map(|v| {
                let eth_status = chip.eth_safe();
                let remote = if let Some(wh) = v.as_wh() {
                    println!("{:X}", wh.get_telemetry().unwrap().smbus_tx_board_id_low);

                    wh.is_remote
                } else {
                    false
                };
                (
                    v.get_arch(),
                    remote,
                    status,
                    eth_status,
                )
            })
        );
    }
}
