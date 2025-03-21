pub mod flash_info {
    include!(concat!(env!("OUT_DIR"), "/flash_info.rs"));
}
pub mod fw_table {
    include!(concat!(env!("OUT_DIR"), "/fw_table.rs"));
}
pub mod read_only {
    include!(concat!(env!("OUT_DIR"), "/read_only.rs"));
}
