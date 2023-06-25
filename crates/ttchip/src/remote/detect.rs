use std::collections::HashSet;

use kmdif::PciError;

use crate::{EthCoord, Wormhole};

use super::{get_local_chip_coord, RemoteWormholeChip};

fn get_remote_eth_sys_addr(
    chip: &mut RemoteWormholeChip,
    eth_x: u8,
    eth_y: u8,
    shelf_offset: u64,
    rack_offset: u64,
) -> Result<(EthCoord, (u8, u8)), PciError> {
    let remote_id = chip
        .read32(eth_x, eth_y, false, 0x1100 + (4 * rack_offset))?;
    let remote_rack_x = remote_id & 0xFF;
    let remote_rack_y = (remote_id >> 8) & 0xFF;

    let remote_id = chip
        .read32(eth_x, eth_y, false, 0x1100 + (4 * shelf_offset))?;
    let remote_shelf_x = (remote_id >> 16) & 0x3F;
    let remote_shelf_y = (remote_id >> 22) & 0x3F;

    let remote_noc_x = (remote_id >> 4) & 0x3F;
    let remote_noc_y = (remote_id >> 10) & 0x3F;

    Ok((
        EthCoord {
            shelf_x: remote_shelf_x as u8,
            shelf_y: remote_shelf_y as u8,
            rack_x: remote_rack_x as u8,
            rack_y: remote_rack_y as u8,
        },
        (remote_noc_x as u8, remote_noc_y as u8),
    ))
}

pub fn get_hops(
    chip: &mut RemoteWormholeChip,
    only_unique: bool,
    port_statuses: Vec<(usize, u32)>,
    src_coord: EthCoord,
) -> Result<Vec<(usize, EthCoord, (u8, u8))>, PciError> {
    let eth_locations = [
        (9, 0),
        (1, 0),
        (8, 0),
        (2, 0),
        (7, 0),
        (3, 0),
        (6, 0),
        (4, 0),
        (9, 6),
        (1, 6),
        (8, 6),
        (2, 6),
        (7, 6),
        (3, 6),
        (6, 6),
        (4, 6),
    ];

    let mut output = Vec::new();
    let mut returned_paths = HashSet::new();
    for (eth_id, status) in port_statuses {
        // A status of 0 is an untrained port in an unknown state.
        // A status of 1 is an unconnected port (or a port where training failed).
        if status == 0 || status == 1 {
            continue;
        }

        let (coord, noc_xy) = get_remote_eth_sys_addr(
            chip,
            eth_locations[eth_id].0,
            eth_locations[eth_id].1,
            9,
            10,
        )?;
        let jump_key = (src_coord.clone(), coord);
        if returned_paths.contains(&jump_key) && only_unique {
            continue;
        }
        returned_paths.insert(jump_key);

        output.push((eth_id, coord, noc_xy));
    }

    Ok(output)
}

fn run_workload(
    chip: &mut Wormhole,
    coord: EthCoord,
    workload: &mut impl FnMut(&mut RemoteWormholeChip) -> Result<(), PciError>,
    seen_chips: &mut HashSet<(EthCoord, u64)>,
) -> Result<(), PciError> {
    let mut remote = chip.remote(coord)?;

    // board_id = (wormhole.AXI.read32("ARC_CSM.SPI_TABLE.board_info[1]") << 32) | wormhole.AXI.read32("ARC_CSM.SPI_TABLE.board_info[0]")
    let board_id = ((remote.read32(0, 10, false, 0x8_1000_0000 + 0x78828 + 0x10C)? as u64) << 32)
        | remote.read32(0, 10, false, 0x8_1000_0000 + 0x78828 + 0x108)? as u64;

    let chip_key = (coord.clone(), board_id);

    if seen_chips.contains(&chip_key) {
        return Ok(());
    }

    seen_chips.insert(chip_key);

    workload(&mut remote)?;

    // let port_status = [wormhole.NOC.read(9, 0, link_base + (i * 4)) for i in range(16)]
    let mut port_status = Vec::new();
    for i in 0..16 {
        port_status.push((i as usize, remote.read32(9, 0, false, 0x1200 + (i * 4))?));
    }

    let mut hops = get_hops(&mut remote, true, port_status, coord)?;
    hops.sort_by_key(|v| v.0);

    for (eth_id, coord, next_noc_xy) in hops {
        run_workload(chip, coord, workload, seen_chips)?;
    }

    Ok(())
}

pub fn run_on_all_chips(
    chip: &mut Wormhole,
    seen_chips: Option<HashSet<(EthCoord, u64)>>,
    mut workload: impl FnMut(&mut RemoteWormholeChip) -> Result<(), PciError>,
) -> Result<HashSet<(EthCoord, u64)>, PciError> {
    let mut seen_chips = seen_chips.unwrap_or(HashSet::new());
    let local_coord = get_local_chip_coord(chip)?;

    run_workload(chip, local_coord, &mut workload, &mut seen_chips)?;

    Ok(seen_chips)
}
