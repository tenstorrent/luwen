//! flash_glx_revc_board_id: Check if UBB is rev C (bits [35:32] of board ID >= 3).
//! If rev C, announce and exit. If not, flash board ID to set bits [35:32] to 3 and
//! re-write boardcfg (only board_id changed). Supports --dry_run to show would-be board ID.

use clap::Parser as _;
use luwen::pci::detect_chips;
use luwen_api::chip::spirom_tables;
use luwen_api::chip::Blackhole;
use prost::Message;
use serde_json::Value;
use std::collections::HashMap;

/// Bits [35:32] of board ID == 3 indicates UBB revC (new cable cartridge).
const REVC_NIBBLE: u64 = 3;
const REVC_NIBBLE_SHIFT: u32 = 32;
const REVC_NIBBLE_MASK: u64 = 0xf;
#[derive(clap::Parser)]
struct Args {
    #[arg(long, help = "Don't flash; only print what the new board ID would be")]
    dry_run: bool,

    #[arg(long, help = "Only operate on a single device by index.")]
    index: Option<u32>,
}

const TAG_NAME: &str = "boardcfg";

/// Returns true if board_id indicates UBB revC (bits [35:32] >= 3).
fn is_revc_ubb(board_id: u64) -> bool {
    ((board_id >> REVC_NIBBLE_SHIFT) & REVC_NIBBLE_MASK) >= REVC_NIBBLE
}

/// Set bits [35:32] of board_id to 3; leave all other bits unchanged.
fn board_id_with_revc_nibble(board_id: u64) -> u64 {
    let low_32 = board_id & 0xffff_ffff;
    let high_32 = (board_id >> 32) as u32;
    let new_high_32 = (high_32 & !REVC_NIBBLE_MASK as u32) | REVC_NIBBLE as u32;
    println!("New high 32: 0x{:08x}", new_high_32);
    ((new_high_32 as u64) << 32) | low_32
    // TEST_BOARD_ID
}

fn get_active_board_cfg_info(chip: &Blackhole) -> Result<(u32, usize), Box<dyn std::error::Error>> {
    let tag_info = chip
        .get_boot_fs_tables_spi_read(TAG_NAME)?
        .ok_or("Couldn't find boardcfg on chip")?;

    let spi_addr = tag_info.1.spi_addr;
    let image_size = tag_info.1.flags.image_size() as usize;

    Ok((spi_addr, image_size))
}

fn decode_boot_fs_table(
    chip: &Blackhole,
    tag_name: &str,
    spi_addr: u32,
    original_image_size: usize,
) -> Result<HashMap<String, Value>, Box<dyn std::error::Error>> {
    let image_size = original_image_size;
    let mut proto_bin = vec![0u8; image_size];
    chip.spi_read(spi_addr, &mut proto_bin)?;

    let final_decode_map: HashMap<String, Value>;
    proto_bin = spirom_tables::remove_padding_proto_bin(&proto_bin)?.to_vec();

    if tag_name == "cmfwcfg" || tag_name == "origcfg" {
        final_decode_map =
            spirom_tables::to_hash_map(spirom_tables::fw_table::FwTable::decode(&*proto_bin)?);
    } else if tag_name == "boardcfg" {
        final_decode_map =
            spirom_tables::to_hash_map(spirom_tables::read_only::ReadOnly::decode(&*proto_bin)?);
    } else if tag_name == "flshinfo" {
        final_decode_map = spirom_tables::to_hash_map(
            spirom_tables::flash_info::FlashInfoTable::decode(&*proto_bin)?,
        );
    } else {
        return Err(format!("Unsupported tag name: {tag_name}").into());
    };
    Ok(final_decode_map)
}

fn run_device(bh: &Blackhole, dry_run: bool) -> Result<(), Box<dyn std::error::Error>> {
    // Read board ID from boardcfg
    let (active_boardcfg_addr, active_boardcfg_size) = get_active_board_cfg_info(bh)?;
    let active_boardcfg =
        decode_boot_fs_table(bh, TAG_NAME, active_boardcfg_addr, active_boardcfg_size)?;

    println!("Active boardcfg before: {active_boardcfg:#?}");
    let board_id = active_boardcfg.get("board_id").unwrap().as_u64().unwrap();
    println!(
        "Board ID from boardcfg: 0x{:08x}_{:08x}",
        (board_id >> 32) as u32,
        board_id as u32
    );

    // Check if board is bh-glx and if not quit
    if (board_id >> 36) & 0xFFFFF != 0x47 {
        println!("Board type: 0x{:x}", (board_id >> 36) & 0xFFFFF);
        println!("Board is not a bh-glx, skipping");
        return Ok(());
    }

    // If board is already rev C, quit
    if is_revc_ubb(board_id) {
        println!(
            "Galaxy rev C UBB detected (bits [35:32] = {} >= 3). New cable cartridge. No action needed.",
            (board_id >> REVC_NIBBLE_SHIFT) & REVC_NIBBLE_MASK
        );
        return Ok(());
    }

    let new_board_id = board_id_with_revc_nibble(board_id);
    println!(
        "Not rev C (bits [35:32] = {}). Target board ID with rev C nibble: 0x{:08x}_{:08x}",
        (board_id >> REVC_NIBBLE_SHIFT) & REVC_NIBBLE_MASK,
        (new_board_id >> 32) as u32,
        new_board_id as u32
    );

    if dry_run {
        println!(
            "[dry_run] Would set board_id to 0x{:08x}_{:08x} (no flash performed).",
            (new_board_id >> 32) as u32,
            new_board_id as u32
        );
        return Ok(());
    }

    flash_boardcfg(bh, new_board_id)?;
    Ok(())
}

fn flash_boardcfg(bh: &Blackhole, board_id: u64) -> Result<(), Box<dyn std::error::Error>> {
    let (active_boardcfg_addr, active_boardcfg_size) = get_active_board_cfg_info(bh)?;
    let mut active_boardcfg =
        decode_boot_fs_table(bh, TAG_NAME, active_boardcfg_addr, active_boardcfg_size)?;

    active_boardcfg.insert("board_id".to_string(), Value::from(board_id));
    println!("Active boardcfg: {active_boardcfg:#?}");

    bh.encode_and_write_boot_fs_table(active_boardcfg, TAG_NAME)?;
    println!(
        "Flashed boardcfg with new board_id 0x{:08x}_{:08x}.",
        (board_id >> 32) as u32,
        board_id as u32
    );

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
        // // test only on first device
        // if idx != 0 {
        //     continue;
        // }
        if let Some(index) = args.index {
            if index != idx as u32 {
                continue;
            }
        }

        println!("=== Device {} ===", idx);

        if let Some(bh) = device.as_bh() {
            match run_device(bh, args.dry_run) {
                Ok(()) => {}
                Err(e) => println!("Device {} error: {}", idx, e),
            }
        } else {
            println!("Device {} is not a Blackhole chip, skipping", idx);
        }
    }

    Ok(())
}
