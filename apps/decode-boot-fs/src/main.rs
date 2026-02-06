use luwen::pci::detect_chips;
use luwen_api::chip::spirom_tables;
use prost::Message;
use serde_json::Value;
use std::collections::HashMap;

const TAG_NAME: &str = "boardcfg";
const SPI_ADDR_1: u32 = 0xfff000;
const SPI_ADDR_2: u32 = 0x3fff000;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let devices = detect_chips()?;
    if devices.is_empty() {
        eprintln!("No chips detected");
        return Err("No chips found".into());
    }
    println!("Decoding boot FS table: {}", TAG_NAME);
    println!("Found {} device(s)", devices.len());
    println!();


    // Parse hex string to u32
    // Process all detected devices
    for (idx, device) in devices.iter().enumerate() {
        if idx != 0 {
            continue;
        }

        println!("=== Device {} ===", idx + 1);
        // Decode the boot FS table
        if let Some(bh) = device.as_bh() {
            let (original_spi_addr, original_image_size) = get_original_board_cfg_info(bh)?;
            println!("Original boardcfg SPI address: 0x{:08x}", original_spi_addr);
            println!("Original boardcfg image size: {} bytes", original_image_size);

            // Take the first valid boardcfg from either address
            let config_to_flash = get_valid_boardcfg_at_addr(bh, SPI_ADDR_1, original_image_size)
                .ok()
                .flatten()
                .or_else(|| get_valid_boardcfg_at_addr(bh, SPI_ADDR_2, original_image_size).ok().flatten());

            if let Some(config) = config_to_flash {
                println!("Flashing boardcfg to 0x{:08x}...", original_spi_addr);
                bh.encode_and_write_boot_fs_table(config, TAG_NAME)?;
                println!("Successfully flashed boardcfg to 0x{:08x}", original_spi_addr);
            } else {
                println!("No valid boardcfg found at 0x{:08x} or 0x{:08x} - not flashing", SPI_ADDR_1, SPI_ADDR_2);
            }

        }
    }

    Ok(())
}

fn get_valid_boardcfg_at_addr(
    chip: &luwen_api::chip::Blackhole,
    spi_addr: u32,
    original_image_size: usize,
) -> Result<Option<HashMap<String, Value>>, Box<dyn std::error::Error>> {
    let boardcfg = match decode_boot_fs_table(chip, TAG_NAME, spi_addr, original_image_size) {
        Ok(decoded) => {
            println!("Successfully decoded {} at 0x{:08x}:", TAG_NAME, spi_addr);
            println!("{}", serde_json::to_string_pretty(&decoded)?);
            Some(decoded)
        }
        Err(e) => {
            eprintln!("Failed to decode {} at 0x{:08x}: {}", TAG_NAME, spi_addr, e);
            None
        }
    };

    let extract_ids = |config: &HashMap<String, Value>| -> (u32, u64) {
        let vendor_id = config
            .get("vendor_id")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;
        let board_id = config
            .get("board_id")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        (vendor_id, board_id)
    };

    let (vendor_id, board_id) = boardcfg.as_ref().map(extract_ids).unwrap_or((0, 0));
    println!("Config at 0x{:08x}: vendor_id={}, board_id={}", spi_addr, vendor_id, board_id);

    let has_valid_ids = vendor_id != 0 && board_id != 0;
    Ok(if has_valid_ids { boardcfg } else { None })
}

fn get_original_board_cfg_info(
    chip: &luwen_api::chip::Blackhole,
) -> Result<(u32, usize), Box<dyn std::error::Error>> {
    let tag_info = chip
        .get_boot_fs_tables_spi_read("boardcfg")?
        .ok_or_else(|| "Couldn't find boardcfg on chip")?;
    
    let spi_addr = tag_info.1.spi_addr;
    let image_size = tag_info.1.flags.image_size() as usize;
    
    Ok((spi_addr, image_size))
}

fn decode_boot_fs_table(
    chip: &luwen_api::chip::Blackhole,
    tag_name: &str,
    spi_addr: u32,
    original_image_size: usize,
) -> Result<HashMap<String, Value>, Box<dyn std::error::Error>> {
    // Return the decoded boot fs table as a HashMap
    // Get the spi address and image size of the tag and read the proto bin
    // Decode the proto bin and convert it to a HashMap
    let image_size = original_image_size;
    println!("  spi_addr: 0x{:08x}", spi_addr);
    println!("  image_size: {} bytes", image_size);
    println!();

    // declare as vec to allow non-const size
    let mut proto_bin = vec![0u8; image_size as usize];
    chip.spi_read(spi_addr, &mut proto_bin)?;
    
    let final_decode_map: HashMap<String, Value>;
    // remove padding
    proto_bin = spirom_tables::remove_padding_proto_bin(&proto_bin)?.to_vec();

    if tag_name == "cmfwcfg" || tag_name == "origcfg" {
        final_decode_map =
            spirom_tables::to_hash_map(spirom_tables::fw_table::FwTable::decode(&*proto_bin)?);
    } else if tag_name == "boardcfg" {
        final_decode_map = spirom_tables::to_hash_map(
            spirom_tables::read_only::ReadOnly::decode(&*proto_bin)?,
        );
    } else if tag_name == "flshinfo" {
        final_decode_map = spirom_tables::to_hash_map(
            spirom_tables::flash_info::FlashInfoTable::decode(&*proto_bin)?,
        );
    } else {
        return Err(format!("Unsupported tag name: {tag_name}").into());
    };
    Ok(final_decode_map)
}

