use clap::Parser;
use luwen::pci::detect_chips;
use luwen_api::chip::spirom_tables;
use prost::Message;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Parser, Debug)]
#[command(author, version, about = "Decode boot filesystem tables from SPI flash")]
struct Args {
    /// Tag name to decode (boardcfg, flshinfo, cmfwcfg, origcfg)
    #[arg(short, long, default_value = "boardcfg")]
    tag: String,
    /// SPI address to read from (hex string, e.g., 0xfff000 or fff000)
    #[arg(short, long, default_value = "0")]
    spi_addr: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let devices = detect_chips()?;

    if devices.is_empty() {
        eprintln!("No chips detected");
        return Err("No chips found".into());
    }

    // Use the first detected chip
    let device = &devices[0];

    println!("Decoding boot FS table: {}", args.tag);
    println!();

    // Parse hex string to u32
    let spi_addr = if args.spi_addr.starts_with("0x") || args.spi_addr.starts_with("0X") {
        u32::from_str_radix(&args.spi_addr[2..], 16)
    } else {
        u32::from_str_radix(&args.spi_addr, 16)
    }.map_err(|e| format!("Invalid hex address '{}': {}", args.spi_addr, e))?;

    // Decode the boot FS table
    if let Some(bh) = device.as_bh() {
        match decode_boot_fs_table(bh, &args.tag, spi_addr) {
            Ok(decoded) => {
                println!("Successfully decoded {}:", args.tag);
                println!("{}", serde_json::to_string_pretty(&decoded)?);
            }
            Err(e) => {
                eprintln!("Failed to decode {}: {}", args.tag, e);
                return Err(e);
            }
        }
    } else if let Some(wh) = device.as_wh() {
        eprintln!("Wormhole chips don't support boot FS table decoding");
        return Err("Unsupported chip type".into());
    } else {
        return Err("Unsupported chip type".into());
    }

    Ok(())
}

fn decode_boot_fs_table(
    chip: &luwen_api::chip::Blackhole,
    tag_name: &str,
    spi_addr: u32,
) -> Result<HashMap<String, Value>, Box<dyn std::error::Error>> {
    // Return the decoded boot fs table as a HashMap
    // Get the spi address and image size of the tag and read the proto bin
    // Decode the proto bin and convert it to a HashMap
    let tag_info = chip
        .get_boot_fs_tables_spi_read(tag_name)?
        .ok_or_else(|| format!("Tag '{tag_name}' not found in boot FS tables"))?;
    
    // Use provided spi_addr if non-zero, otherwise use the one from tag_info
    let spi_addr = if spi_addr != 0 {
        spi_addr
    } else {
        tag_info.1.spi_addr
    };
    let image_size = 12;
    // tag_info.1.flags.image_size();
    // 

    println!("Found tag '{}' in boot FS:", tag_name);
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

