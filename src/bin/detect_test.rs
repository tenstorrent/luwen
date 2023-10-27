fn main() {
    let partial_chips = match luwen_ref::detect_chips() {
        Ok(chips) => chips,
        Err(err) => panic!("{}", err),
    };

    for chip in partial_chips {
        println!(
            "Chip: {:?}",
            chip.try_upgrade().map(|v| {
                let remote = if let Some(wh) = v.as_wh() {
                    wh.is_remote
                } else {
                    false
                };
                (
                    remote,
                    v.inner.get_telemetry().map(|v| format!("{:X}", v.board_id)),
                )
            })
        );
    }
}
