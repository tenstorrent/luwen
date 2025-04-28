use luwen_if::ChipImpl;

fn main() {
    let interfaces = luwen_ref::PciDevice::scan();
    eprintln!("Found {} chips to check", interfaces.len());

    let mut output = String::new();
    output.push_str("{\n");
    for interface in interfaces.iter().copied() {
        // Just try to reset the link... if it fails then we should still try stuff
        let chip = luwen_ref::open(interface);
        let chip = match chip {
            Ok(chip) => chip,
            Err(err) => {
                eprintln!("Error: Hit error {err:?} while trying to open {interface}");
                continue;
            }
        };

        let chip = match chip.as_bh() {
            Some(chip) => chip,
            None => {
                eprintln!("Error: {interface} not BH it is {}", chip.get_arch());
                continue;
            }
        };

        let telem = match chip.get_telemetry() {
            Ok(telem) => telem,
            Err(err) => {
                eprintln!("Error: failed to get telemetry from {interface} instead hit {err:?}");
                continue;
            }
        };

        output.push_str(&format!("\t\"{interface}\": {{\n"));
        output.push_str(&format!(
            "\t\t\"enabled tensix\": {},\n",
            telem.tensix_enabled_col
        ));
        output.push_str(&format!("\t\t\"enabled eth\": {},\n", telem.enabled_eth));
        output.push_str(&format!("\t\t\"enabled gddr\": {},\n", telem.enabled_gddr));
        output.push_str(&format!(
            "\t\t\"enabled l2cpu\": {},\n",
            telem.enabled_l2cpu
        ));
        output.push_str(&format!("\t\t\"enabled pcie\": {}\n", telem.enabled_pcie));
        output.push_str("\t}},\n");
    }
    output.pop();
    output.pop();
    output.push_str("\n}");
    print!("{}", output);
}
