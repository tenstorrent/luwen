use luwen_if::ChipImpl;

fn main() {
    for chip in luwen_ref::detect_chips_fallible().unwrap() {
        if let Some(chip) = chip.try_upgrade() {
            print!("{}", chip.get_arch());
            if let Some(true) = chip.as_wh().map(|v| v.is_remote) {
                println!(" (remote)");
            } else {
                println!();
            }
            if let Ok(telem) = chip.get_telemetry() {
                println!("\t{:x}", telem.board_id);
            }
            println!("-----");
        }
    }
}
