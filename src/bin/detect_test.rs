fn main() {
    let partial_chips = luwen_ref::detect_chips().unwrap();

    for chip in partial_chips {
        println!("Chip: {:?}", chip.status());
    }
}
