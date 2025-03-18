use luwen_if::ChipImpl;

fn main() {
    let chip = match luwen_ref::open(0) {
        Ok(chips) => chips,
        Err(err) => panic!("{}", err),
    };

    println!("Detected enumerated BH; gathering telemetry");

    let telem_a = chip.get_telemetry().unwrap();

    println!("Sleeping for 1 second before checking telemetry again");

    std::thread::sleep(std::time::Duration::from_secs(1));

    println!("Gathering telemetry again");

    let telem_b = chip.get_telemetry().unwrap();

    if telem_a.timer_heartbeat == telem_b.timer_heartbeat {
        panic!("\x1b[0;31m[FAIL]\x1b[0m Detected ARC hang");
    }

    if telem_a.vcore < 700 || telem_a.vcore > 850 {
        panic!(
            "\x1b[0;31m[FAIL]\x1b[0m Board vcore reading is outside of the expected range {}",
            telem_a.vcore
        );
    }

    if telem_a.tdc < 3 || telem_a.tdc > 200 {
        panic!(
            "\x1b[0;31m[FAIL]\x1b[0m Board tdc reading is outside of the expected range {}",
            telem_a.tdc
        );
    }

    println!("\x1b[0;32m[PASS]\x1b[0m");
}
