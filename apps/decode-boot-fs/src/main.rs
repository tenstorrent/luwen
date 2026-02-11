use luwen::pci::detect_chips;
use luwen_api::chip::spirom_tables;
use prost::Message;
use serde_json::Value;
use std::collections::HashMap;
use luwen_api::chip::Blackhole;
use clap::Parser as _;

#[derive(clap::Parser)]
struct Args {
    #[arg(long, help = "Don't actually write to flash, just print what would be done")]
    dry_run: bool,

    #[arg(long, help = "Only attempt to fix a single device.")]
    index: Option<u32>,
}

const TAG_NAME: &str = "boardcfg";
const BOARDCFG_BACKUP_1: u32 = 0x0fff000;
const BOARDCFG_BACKUP_2: u32 = 0x3fff000;

fn fix_board(bh: &Blackhole, dry_run: bool) -> Result<HashMap<String, Value>, Box<dyn std::error::Error>> {
    let (active_boardcfg_addr, active_boardcfg_size) = get_active_board_cfg_info(bh)?;
    println!("Active boardcfg address: 0x{:x}, size: {}", active_boardcfg_addr, active_boardcfg_size);

    let mut active_boardcfg
        = decode_boot_fs_table(bh, TAG_NAME, active_boardcfg_addr, active_boardcfg_size)
        .map(|cfg| { println!("Found active boardcfg with board_id: {:x}", cfg.get("board_id").unwrap().as_u64().unwrap()); cfg })
        .unwrap_or_else(|e| {
            println!("Failed to decode active boardcfg: {}, creating new from scratch.", e);

            let mut cfg = HashMap::new();
            cfg.insert("board_id".to_string(), Value::from(0)); // Placeholder, will be overwritten if we find a valid backup
            cfg.insert("vendor_id".to_string(), Value::from(0x1E52));
            cfg.insert("asic_location".to_string(), Value::from(0));
            cfg
        });

    let backup_board_id_1 = get_boardid_from_addr(bh, BOARDCFG_BACKUP_1)?;
    if backup_board_id_1.is_some() {
        println!("Found board ID 0x{:x} at backup address 0x{:x}", backup_board_id_1.unwrap(), BOARDCFG_BACKUP_1);
    }

    let backup_board_id_2 = get_boardid_from_addr(bh, BOARDCFG_BACKUP_2)?;
    if backup_board_id_2.is_some() {
        println!("Found board ID 0x{:x} at backup address 0x{:x}", backup_board_id_2.unwrap(), BOARDCFG_BACKUP_2);
    }

    if backup_board_id_1.is_none() && backup_board_id_2.is_none() {
        return Err("No valid board ID found in backup addresses, not modifying boardcfg.".into());
    }

    let backup_board_id = backup_board_id_1.or(backup_board_id_2).unwrap();
    active_boardcfg.insert("board_id".to_string(), Value::from(backup_board_id));

    if !dry_run {
        bh.encode_and_write_boot_fs_table(active_boardcfg.clone(), TAG_NAME)?;
    }

    Ok(active_boardcfg)
}

fn check_boardcfg(bh: &Blackhole, flashed_config: Option<HashMap<String, Value>>) -> Result<(), Box<dyn std::error::Error>> {
    let (active_boardcfg_addr, active_boardcfg_size) = get_active_board_cfg_info(bh)?;
    println!("Active boardcfg address: 0x{:x}, size: {}", active_boardcfg_addr, active_boardcfg_size);

    // Always read and display boardcfg from flash (whether we flashed or not)
    println!();
    println!("=== Reading boardcfg from flash at 0x{:08x} ===", active_boardcfg_addr);
    match decode_boot_fs_table(bh, TAG_NAME, active_boardcfg_addr, active_boardcfg_size) {
        Ok(flash_config) => {
            println!("Successfully read and decoded boardcfg from 0x{:08x}:", active_boardcfg_addr);
            println!("{}", serde_json::to_string_pretty(&flash_config)?);

            // Extract IDs for comparison if we flashed
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

            if let Some(flashed) = flashed_config {
                let (flashed_vendor_id, flashed_board_id) = extract_ids(&flashed);
                let (flash_vendor_id, flash_board_id) = extract_ids(&flash_config);

                println!();
                println!("Verification summary:");
                println!("  Flashed:  vendor_id={}, board_id={}", flashed_vendor_id, flashed_board_id);
                println!("  From flash: vendor_id={}, board_id={}", flash_vendor_id, flash_board_id);

                if flashed_vendor_id == flash_vendor_id && flashed_board_id == flash_board_id {
                    println!("  ✓ Verification successful: IDs match!");
                } else {
                    println!("  ✗ Verification warning: IDs do not match!");
                }
            } else {
                let (flash_vendor_id, flash_board_id) = extract_ids(&flash_config);
                println!();
                println!("Current boardcfg in flash: vendor_id={}, board_id={}", flash_vendor_id, flash_board_id);
            }
        }
        Err(e) => {
            eprintln!("Failed to read boardcfg from 0x{:08x}: {}", active_boardcfg_addr, e);
        }
    }

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let devices = detect_chips()?;
    if devices.is_empty() {
        println!("No chips detected");
        return Err("No chips found".into());
    }

    println!("Found {} device(s)", devices.len());

    for (idx, device) in devices.iter().enumerate() {

        if let Some(index) = args.index {
            if index != (idx + 1) as u32 {
                continue;
            }
        }

        println!("=== Device {} ===", idx + 1);

        if let Some(bh) = device.as_bh() {
            let mut flashed_config = None;
            let board_result = fix_board(bh, args.dry_run);
            match board_result {
                Ok(fc) => {
                    println!("Successfully fixed device {}: flashed new boardcfg", idx + 1);
                    flashed_config = Some(fc);
                }
                Err(e) => println!("Failed to fix device {}: {}", idx + 1, e),
            }

            match check_boardcfg(bh, flashed_config) {
                Ok(_) => println!("Verified boardcfg for device {}", idx + 1),
                Err(e) => println!("Failed to check boardcfg for device {}: {}", idx + 1, e),
            }
        } else {
            println!("Device {} is not a Blackhole chip, skipping", idx + 1);
        }
    }

    Ok(())
}

fn get_boardid_from_addr(
    chip: &luwen_api::chip::Blackhole,
    spi_addr: u32,
) -> Result<Option<u64>, Box<dyn std::error::Error>> {

    let mut buf = [0u8; 16];
    chip.spi_read(spi_addr, &mut buf)?;

    // We expect to see a protobuf message starting with 08 (field 1, VARINT).
    // This is uint64 board_id = 1;
    if buf[0] != 0x08 {
        return Ok(None);
    }

    let mut board_id: u64 = 0;
    let mut shift = 0;
    for i in 1..buf.len() {
        let byte = buf[i];

        board_id |= ((byte & 0x7F) as u64) << shift;
        shift += 7;

        if (byte & 0x80) == 0 {
            break;
        } else if i == buf.len() - 1 {
            // If we reach the end of the buffer and the last byte still has the continuation bit
            // set, then this was never a valid board id.
            return Ok(None);
        }
    }

    // TODO: Sanity-check board ID.

    return Ok(Some(board_id));
}

fn get_active_board_cfg_info(
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
    // println!("  spi_addr: 0x{:08x}", spi_addr);
    // println!("  image_size: {} bytes", image_size);
    // println!();

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
