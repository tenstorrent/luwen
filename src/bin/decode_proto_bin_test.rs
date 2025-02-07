use luwen_ref::detect_chips;

fn main() {
    let devices = detect_chips().unwrap();
    let bh = devices[0].as_bh();
    println!();
    if let Some(bh) = bh {
        // Test decoding the bin from the boot fs
        let decode_msg = bh.decode_boot_fs_table("boardcfg");
        println!("Decoded boardcfg: {:#?}", decode_msg);
        println!();

        let decode_msg = bh.decode_boot_fs_table("flshinfo");
        println!("Decoded flshinfo: {:#?}", decode_msg);
        println!();

        let decode_msg = bh.decode_boot_fs_table("cmfwcfg");
        println!("Decoded cmfwcfg: {:#?}", decode_msg);
        println!();

        let decode_msg = bh.decode_boot_fs_table("origcfg");
        println!("Decoded origcfg: {:#?}", decode_msg);
        println!();
    }
}
