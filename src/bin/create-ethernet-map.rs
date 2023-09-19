use std::collections::{HashMap, HashSet};

use luwen_core::Arch;
use ttchip::{remote::RemoteWormholeChip, EthCoord};

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ChipIdent {
    pub arch: Arch,
    pub board_id: Option<u64>,
    pub interface: Option<usize>,
    pub coord: Option<EthCoord>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut seen_chips = HashSet::new();

    let mut chips = HashMap::new();
    let mut mmio_chips = Vec::new();
    let mut connection_map = HashMap::new();

    for chip in ttchip::scan()? {
        match chip {
            ttchip::AllChips::Wormhole(mut wh) => {
                let ident = ChipIdent {
                    arch: Arch::Wormhole,
                    board_id: Some(wh.board_id()?),
                    interface: None,
                    coord: Some(wh.coord()?),
                };
                if !chips.contains_key(&ident) {
                    chips.insert(ident.clone(), chips.len());
                }
                mmio_chips.push((ident, wh.chip.transport.id));
                seen_chips = ttchip::run_on_all_chips(&mut wh, Some(seen_chips), |chip| {
                    let ident = ChipIdent {
                        arch: Arch::Wormhole,
                        board_id: Some(chip.board_id()?),
                        interface: None,
                        coord: Some(chip.coord.clone()),
                    };

                    if !chips.contains_key(&ident) {
                        chips.insert(ident.clone(), chips.len());
                    }

                    let mut port_status = Vec::new();
                    for i in 0..16 {
                        port_status.push((i as usize, chip.read32(9, 0, false, 0x1200 + (i * 4))?));
                    }

                    let mut hops = ttchip::remote::detect::get_hops(
                        chip,
                        false,
                        port_status,
                        chip.coord.clone(),
                    )?;
                    hops.sort_by_key(|v| v.0);

                    let mut connection_info: HashMap<_, Vec<_>> = HashMap::new();
                    for (local_eth_id, coord, next_noc_xy) in hops {
                        let mut next = RemoteWormholeChip::create(
                            chip.chip,
                            chip.eth_x,
                            chip.eth_y,
                            false,
                            std::time::Duration::from_secs(11),
                            coord,
                        )?;

                        let next_ident = ChipIdent {
                            arch: Arch::Wormhole,
                            board_id: Some(next.board_id()?),
                            interface: None,
                            coord: Some(coord),
                        };

                        connection_info
                            .entry(next_ident)
                            .or_default()
                            .push((local_eth_id, next_noc_xy));
                    }
                    connection_map.insert(ident, connection_info);

                    Ok(())
                })?;
            }
            ttchip::AllChips::Grayskull(gs) => {
                let ident = ChipIdent {
                    arch: Arch::Grayskull,
                    board_id: None,
                    interface: Some(gs.chip.transport.id),
                    coord: None,
                };
                chips.insert(ident.clone(), chips.len());
                mmio_chips.push((ident, gs.chip.transport.id));
            }
        }
    }

    let eth_locations = [
        (9u8, 0u8),
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

    let mut connections = Vec::new();

    let mut ident_order = Vec::new();
    for chip in &chips {
        ident_order.push(chip.clone());
    }
    ident_order.sort_by_key(|v| v.1);
    let ident_order: Vec<_> = ident_order.into_iter().map(|v| v.0.clone()).collect();

    let mut known_connections = HashSet::new();
    for chip in &ident_order {
        if let Some(connection_info) = connection_map.get(&chip) {
            for (remote_chip, connection) in connection_info {
                for (local_eth_id, next_noc_xy) in connection {
                    let next_eth_id = eth_locations.iter().position(|v| v == next_noc_xy).unwrap();

                    let local = (chips[chip], *local_eth_id);
                    let remote = (chips[remote_chip], next_eth_id);

                    let first = local.min(remote);
                    let second = local.max(remote);

                    let connection_ident = (first, second);
                    if known_connections.contains(&connection_ident) {
                        continue;
                    }
                    known_connections.insert(connection_ident);

                    connections.push(connection_ident);
                }
            }
        }
    }

    connections.sort();

    println!("ARCH");
    for chip in &ident_order {
        let id = chips[chip];
        println!("{}: {:?}", id, chip.arch);
    }

    println!("REMOTE CHIPS");
    for chip in &ident_order {
        let id = chips[chip];
        if let Some(coord) = &chip.coord {
            println!("{}: {}", id, coord);
        }
    }

    println!("CONNECTIONS");
    for ((local_chip, local_port), (remote_chip, remote_port)) in connections {
        println!("[{local_chip}; {local_port}] -> [{remote_chip}; {remote_port}]");
    }

    println!("MMIO");
    mmio_chips.sort_by_key(|v| v.1);

    for (mmio, interface) in mmio_chips {
        println!("{} -> {}", chips[&mmio], interface);
    }

    Ok(())
}
