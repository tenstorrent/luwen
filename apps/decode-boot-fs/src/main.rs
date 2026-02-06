use luwen::pci::detect_chips;
use luwen_api::chip::spirom_tables;
use bytemuck::bytes_of;
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
            let mut board_cfg_addr_1: Option<HashMap<String, Value>> = None;
            let mut board_cfg_addr_2: Option<HashMap<String, Value>> = None;
            match decode_boot_fs_table(bh, TAG_NAME, SPI_ADDR_1, original_image_size) {
                Ok(decoded) => {
                    println!("Successfully decoded {}:", TAG_NAME);
                    println!("{}", serde_json::to_string_pretty(&decoded)?);
                    board_cfg_addr_1 = Some(decoded);
                }
                Err(e) => {
                    eprintln!("Failed to decode {}: {}", TAG_NAME, e);
                        // Continue to next device instead of returning
                        
                }
            }
            match decode_boot_fs_table(bh, TAG_NAME, SPI_ADDR_2, original_image_size) {
                Ok(decoded) => {
                    println!("Successfully decoded {}:", TAG_NAME);
                    println!("{}", serde_json::to_string_pretty(&decoded)?);
                    board_cfg_addr_2 = Some(decoded);
                }
                Err(e) => {
                    eprintln!("Failed to decode {}: {}", TAG_NAME, e);
                    
                }
            }
            // Check the board configs at both locations. 
            // If vendorID and board ID are not 0, then flash into the chip at the original address. 
            // If they match, then flash anyone into the correct spot. 
            // If not, then only flash the one that is not none.
            
            // Helper to extract vendor_id and board_id from a config HashMap
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
            
            let (vendor_id_1, board_id_1) = board_cfg_addr_1.as_ref().map(extract_ids).unwrap_or((0, 0));
            let (vendor_id_2, board_id_2) = board_cfg_addr_2.as_ref().map(extract_ids).unwrap_or((0, 0));
            
            println!("Config at 0x{:08x}: vendor_id={}, board_id={}", SPI_ADDR_1, vendor_id_1, board_id_1);
            println!("Config at 0x{:08x}: vendor_id={}, board_id={}", SPI_ADDR_2, vendor_id_2, board_id_2);
            
            // Determine which config to flash and where
            let config_to_flash: Option<&HashMap<String, Value>>;
            let target_spi_addr: u32;
            
            // Check if vendorID and board ID are not 0 in either config
            let has_valid_ids_1 = vendor_id_1 != 0 && board_id_1 != 0;
            let has_valid_ids_2 = vendor_id_2 != 0 && board_id_2 != 0;
            
            if has_valid_ids_1 && has_valid_ids_2 {
                // Both have valid IDs - check if they match
                if vendor_id_1 == vendor_id_2 && board_id_1 == board_id_2 {
                    // They match - flash anyone into the correct spot (original address)
                    println!("Configs match - flashing to original address 0x{:08x}", original_spi_addr);
                    config_to_flash = board_cfg_addr_1.as_ref();
                    target_spi_addr = original_spi_addr;
                } else {
                    // They don't match - flash the one with valid IDs to original address
                    println!("Configs don't match - flashing valid config to original address 0x{:08x}", original_spi_addr);
                    config_to_flash = if has_valid_ids_1 {
                        board_cfg_addr_1.as_ref()
                    } else {
                        board_cfg_addr_2.as_ref()
                    };
                    target_spi_addr = original_spi_addr;
                }
            } else if has_valid_ids_1 {
                // Only addr_1 has valid IDs
                println!("Only config at 0x{:08x} has valid IDs - flashing to original address 0x{:08x}", SPI_ADDR_1, original_spi_addr);
                config_to_flash = board_cfg_addr_1.as_ref();
                target_spi_addr = original_spi_addr;
            } else if has_valid_ids_2 {
                // Only addr_2 has valid IDs
                println!("Only config at 0x{:08x} has valid IDs - flashing to original address 0x{:08x}", SPI_ADDR_2, original_spi_addr);
                config_to_flash = board_cfg_addr_2.as_ref();
                target_spi_addr = original_spi_addr;
            } else {
                // Neither has valid IDs - flash whichever one exists
                // Don't flash anything!
                println!("No valid IDs found for BH chip {idx}- not flashing anything");
                continue;
            }
            
            // Flash the selected config to the target address
            if let Some(_config) = config_to_flash {
                println!("Flashing boardcfg to 0x{:08x}...", target_spi_addr);
                // Note: encode_and_write_boot_fs_table writes to the address from tag_info
                // We need to write to a specific address, so we'll need to handle this differently
                // For now, we'll use encode_and_write_boot_fs_table which writes to the original address
                // If target_spi_addr != original_spi_addr, we'd need a custom write function
                if target_spi_addr == original_spi_addr {
                    bh.encode_and_write_boot_fs_table(_config.clone(), TAG_NAME)?;
                    println!("Successfully flashed boardcfg to 0x{:08x}", target_spi_addr);
                } else {
                    eprintln!("Warning: Cannot flash to non-original address 0x{:08x} (original is 0x{:08x})", target_spi_addr, original_spi_addr);
                    eprintln!("Using encode_and_write_boot_fs_table which writes to original address");
                    bh.encode_and_write_boot_fs_table(_config.clone(), TAG_NAME)?;
                }
            }

        }
    }

    Ok(())
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

