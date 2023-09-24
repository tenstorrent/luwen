use std::collections::{HashMap, HashSet};

use luwen_core::Arch;
use luwen_if::{chip::NeighbouringChip, ChipImpl, EthAddr};
use luwen_ref::error::LuwenError;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ChipIdent {
    pub arch: Arch,
    pub board_id: Option<u64>,
    pub interface: Option<u32>,
    pub coord: Option<EthAddr>,
}

fn main() -> Result<(), LuwenError> {
    let mut chips = HashMap::new();
    let mut mmio_chips = Vec::new();
    let mut connection_map = HashMap::new();

    for chip in luwen_ref::detect_chips()? {
        let telemetry = chip.get_telemetry()?;

        let ident = if let Some(wh) = chip.as_wh() {
            let coord = wh.get_local_chip_coord()?;

            let ident = ChipIdent {
                arch: Arch::Wormhole,
                board_id: Some(telemetry.board_id),
                // interface: wh.get_device_info().map(|v| v.interface_id),
                interface: None,
                coord: Some(coord),
            };

            if !wh.is_remote {
                mmio_chips.push((ident.clone(), wh.get_device_info()?.map(|v| v.interface_id)));
            } else {
                let neighbours = chip.get_neighbouring_chips()?;

                let mut connection_info: HashMap<_, Vec<_>> = HashMap::new();
                for NeighbouringChip {
                    local_noc_addr,
                    remote_noc_addr,
                    eth_addr,
                } in neighbours
                {
                    let next = wh.open_remote(eth_addr)?;

                    let next_ident = ChipIdent {
                        arch: Arch::Wormhole,
                        board_id: Some(next.get_telemetry()?.board_id),
                        // interface: next.get_device_info().map(|v| v.interface_id),
                        interface: None,
                        coord: Some(eth_addr),
                    };

                    connection_info
                        .entry(next_ident)
                        .or_default()
                        .push((local_noc_addr, remote_noc_addr));
                }
                connection_map.insert(ident.clone(), connection_info);
            }

            ident
        } else if let Some(gs) = chip.as_gs() {
            let ident = ChipIdent {
                arch: Arch::Grayskull,
                board_id: None,
                interface: gs.get_device_info()?.map(|v| v.interface_id),
                coord: None,
            };

            chips.insert(ident.clone(), chips.len());
            mmio_chips.push((ident.clone(), gs.get_device_info()?.map(|v| v.interface_id)));

            ident
        } else {
            unimplemented!("Unknown chip type")
        };

        if !chips.contains_key(&ident) {
            chips.insert(ident.clone(), chips.len());
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
                for (local_noc_addr, next_noc_addr) in connection {
                    let current_eth_id = eth_locations
                        .iter()
                        .position(|v| v == local_noc_addr)
                        .unwrap();

                    let next_eth_id = eth_locations
                        .iter()
                        .position(|v| v == next_noc_addr)
                        .unwrap();

                    let local = (chips[chip], current_eth_id);
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
        if let Some(interface) = interface {
            println!("{} -> {}", chips[&mmio], interface);
        }
    }

    Ok(())
}
