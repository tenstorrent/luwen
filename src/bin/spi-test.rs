use luwen_ref::detect_chips;

fn main() {
    let devices = detect_chips().unwrap();

    let addr = 0x20108;

    for device in devices {
        if let Some(wh) = device.as_wh() {
            let spare_addr = 0x20134;

            let mut value = [0; 8];
            wh.spi_read(addr, &mut value).unwrap();
            println!("BOARD_INFO: {:016X}", u64::from_le_bytes(value));

            let mut value_write = [0, 0];
            wh.spi_read(spare_addr, &mut value_write).unwrap();
            value_write[0] = value_write[0].wrapping_add(1);
            if value_write[0] == 0 {
                value_write[1] = value_write[1].wrapping_add(1);
            }
            wh.spi_write(spare_addr, &value_write).unwrap();

            let mut value = [0; 8];

            wh.spi_read(spare_addr, &mut value).unwrap();
            println!("SPARE0: {:x}", u64::from_le_bytes(value));
        } else if let Some(gs) = device.as_gs() {
            let spare_addr = 0x201A1;

            let mut value = [0; 8];
            gs.spi_read(addr, &mut value).unwrap();
            println!("BOARD_INFO: {:016X}", u64::from_le_bytes(value));

            let mut value_write = [0, 0];
            gs.spi_read(spare_addr, &mut value_write).unwrap();
            value_write[0] = value_write[0].wrapping_add(1);
            if value_write[0] == 0 {
                value_write[1] = value_write[1].wrapping_add(1);
            }
            gs.spi_write(spare_addr, &value_write).unwrap();

            let mut value = [0; 8];
            gs.spi_read(spare_addr, &mut value).unwrap();
            println!("SPARE2: {:x}", u64::from_le_bytes(value));
        }
    }
}
