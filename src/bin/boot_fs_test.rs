use luwen_ref::detect_chips;

fn main() {
    let devices = detect_chips().unwrap();
    let bh = devices[0].as_bh();
    println!();
    if let Some(bh) = bh {
        // test reading all the tags from the boot fs
        let tag_read = bh.get_boot_fs_tables_spi_read("boardcfg").unwrap();
        println!("boardcfg tag read: {:?}", tag_read);
        println!();

        let tag_read = bh.get_boot_fs_tables_spi_read("flshinfo").unwrap();
        println!("flshinfo tag read: {:?}", tag_read);
        println!();

        let tag_read = bh.get_boot_fs_tables_spi_read("cmfwcfg").unwrap();
        println!("cmfwcfg tag read: {:?}", tag_read);
        println!();

        let tag_read = bh.get_boot_fs_tables_spi_read("origcfg").unwrap();
        println!("origcfg tag read: {:?}", tag_read);
        println!();
    }
}
