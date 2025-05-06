// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, HashSet};

use luwen_core::Arch;
use luwen_if::{
    chip::{ArcMsgOptions, HlComms, NeighbouringChip},
    ChipImpl, EthAddr,
};
use luwen_ref::error::LuwenError;

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::fs::File;
use std::io::Write;

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChipIdent {
    pub arch: Arch,
    pub board_id: Option<u64>,
    pub interface: Option<u32>,
    pub coord: Option<EthAddr>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChipData {
    pub noc_translation_en: bool,
    pub harvest_mask: u32,
    pub boardtype: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct EthernetLink {
    pub chip: usize,
    pub chan: usize,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct EthernetConnection {
    pub source: EthernetLink,
    pub destination: EthernetLink,
    pub routing_enabled: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EthernetMap {
    pub arch: HashMap<usize, Arch>,
    pub chips: HashMap<usize, EthAddr>,
    pub ethernet_connections: Vec<EthernetConnection>,
    pub chips_with_mmio: HashMap<usize, u32>,
    pub harvesting: HashMap<usize, HarvestInfo>,
    pub boardtype: HashMap<usize, Option<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HarvestInfo {
    pub noc_translation: bool,
    pub harvest_mask: u32,
}

pub fn generate_map() -> Result<EthernetMap, LuwenError> {
    let mut chips = HashMap::new();
    let mut chip_data = HashMap::new();
    let mut mmio_chips = Vec::new();
    let mut connection_map = HashMap::new();

    for chip in luwen_ref::detect_chips()? {
        let telemetry = chip.get_telemetry()?;

        let (ident, data) = if let Some(wh) = chip.as_wh() {
            let coord = wh.get_local_chip_coord()?;

            // Magic value referring to the location of the niu_cfg for a DRAM
            let niu_cfg = wh.noc_read32(0, 0, 0, 0x1000A0000 + 0x100).unwrap();
            let noc_translation_en = (niu_cfg & (1 << 14)) != 0;

            let result = wh
                .arc_msg(ArcMsgOptions {
                    msg: luwen_if::ArcMsg::Typed(luwen_if::TypedArcMsg::GetHarvesting),
                    ..Default::default()
                })
                .unwrap();
            let harvest_mask = match result {
                luwen_if::ArcMsgOk::Ok { rc: _, arg } => arg,
                luwen_if::ArcMsgOk::OkNoWait => unreachable!(),
            };

            let ident = ChipIdent {
                arch: Arch::Wormhole,
                board_id: Some(telemetry.board_id),
                // interface: wh.get_device_info().map(|v| v.interface_id),
                interface: None,
                coord: Some(coord),
            };

            let data = ChipData {
                noc_translation_en,
                harvest_mask,
                boardtype: telemetry.try_board_type().map(|v| v.to_string()),
            };

            if !wh.is_remote {
                mmio_chips.push((ident.clone(), wh.get_device_info()?.map(|v| v.interface_id)));
            }

            let neighbours = wh.get_neighbouring_chips()?;

            let mut connection_info: HashMap<_, Vec<_>> = HashMap::new();
            for NeighbouringChip {
                local_noc_addr,
                remote_noc_addr,
                eth_addr,
                routing_enabled,
            } in neighbours
            {
                if !routing_enabled {
                    continue;
                }

                let next = wh.open_remote(eth_addr)?;

                let next_ident = ChipIdent {
                    arch: Arch::Wormhole,
                    board_id: Some(next.get_telemetry()?.board_id),
                    // interface: next.get_device_info().map(|v| v.interface_id),
                    interface: None,
                    coord: Some(eth_addr),
                };

                let local_id = wh
                    .eth_locations
                    .iter()
                    .position(|v| (v.x, v.y) == local_noc_addr)
                    .unwrap();

                let remote_id = next
                    .eth_locations
                    .iter()
                    .position(|v| (v.x, v.y) == remote_noc_addr)
                    .unwrap();

                connection_info.entry(next_ident).or_default().push((
                    local_id,
                    remote_id,
                    routing_enabled,
                ));
            }
            connection_map.insert(ident.clone(), connection_info);

            (ident, data)
        } else if let Some(gs) = chip.as_gs() {
            let result = gs
                .arc_msg(ArcMsgOptions {
                    msg: luwen_if::ArcMsg::Typed(luwen_if::TypedArcMsg::GetHarvesting),
                    ..Default::default()
                })
                .unwrap();
            let harvest_mask = match result {
                luwen_if::ArcMsgOk::Ok { arg, .. } => arg,
                luwen_if::ArcMsgOk::OkNoWait => unreachable!(),
            };

            let ident = ChipIdent {
                arch: Arch::Grayskull,
                board_id: None,
                interface: gs.get_device_info()?.map(|v| v.interface_id),
                coord: None,
            };

            let data = ChipData {
                noc_translation_en: false,
                harvest_mask,
                boardtype: telemetry.try_board_type().map(|v| v.to_string()),
            };

            mmio_chips.push((ident.clone(), gs.get_device_info()?.map(|v| v.interface_id)));

            (ident, data)
        } else if let Some(bh) = chip.as_bh() {
            let ident = ChipIdent {
                arch: Arch::Blackhole,
                board_id: None,
                interface: bh.get_device_info()?.map(|v| v.interface_id),
                coord: None,
            };

            let data = ChipData {
                noc_translation_en: false,
                harvest_mask: 0,
                boardtype: telemetry.try_board_type().map(|v| v.to_string()),
            };

            mmio_chips.push((ident.clone(), bh.get_device_info()?.map(|v| v.interface_id)));

            (ident, data)
        } else {
            unimplemented!("Unknown chip type")
        };

        if !chips.contains_key(&ident) {
            chip_data.insert(ident.clone(), data);
            chips.insert(ident.clone(), chips.len());
        }
    }

    let mut connections = Vec::new();

    let mut ident_order = Vec::new();
    for chip in &chips {
        ident_order.push(chip);
    }
    ident_order.sort_by_key(|v| v.1);
    let ident_order: Vec<_> = ident_order.into_iter().map(|v| v.0.clone()).collect();

    let mut arch_map = HashMap::new();
    let mut chips_map = HashMap::new();
    let mut chips_with_mmio_map = HashMap::new();
    let mut harvesting_map = HashMap::new();
    let mut boardtype_map = HashMap::new();
    

    let mut known_connections = HashSet::new();
    for chip in &ident_order {
        if let Some(connection_info) = connection_map.get(chip) {
            let id = chips[chip];
            arch_map.insert(id, chip.arch.clone());

            if let Some(coord) = &chip.coord {
                chips_map.insert(id, *coord);
            }
            
            let data = &chip_data[chip];
            harvesting_map.insert(id, HarvestInfo {
                noc_translation: data.noc_translation_en,
                harvest_mask: data.harvest_mask,
            });
            
            boardtype_map.insert(id, data.boardtype.clone());

            for (remote_chip, connection) in connection_info {
                for (current_eth_id, next_eth_id, routing_enabled) in connection {
                    let local = (chips[chip], current_eth_id);
                    let remote = (chips[remote_chip], next_eth_id);

                    let first = local.min(remote);
                    let second = local.max(remote);

                    let connection_ident = (first, second);
                    if known_connections.contains(&connection_ident) {
                        continue;
                    }
                    known_connections.insert(connection_ident);

                    connections.push(EthernetConnection{source: EthernetLink{chip: first.0, chan: *first.1}, 
                        destination: EthernetLink{chip: second.0, chan: *second.1},
                    routing_enabled: *routing_enabled
                    });
                }
            }
        }
    }

    connections.sort();

    for (mmio, interface) in &mmio_chips {
        if let Some(interface) = interface {
            chips_with_mmio_map.insert(chips[mmio], *interface);
        }
    }
    
    // Create the map
    let ethernet_map = EthernetMap {
        arch: arch_map,
        chips: chips_map,
        ethernet_connections: connections,
        chips_with_mmio: chips_with_mmio_map,
        harvesting: harvesting_map,
        boardtype: boardtype_map,
    };
    
    Ok(ethernet_map)
}

pub fn write_ethernet_map<W: Write>(
    writer: W,
    map: &EthernetMap,
) -> Result<(), LuwenError> {
    serde_yaml::to_writer(writer, map)
        .map_err(|e| LuwenError::Custom(format!("Failed to write YAML: {e}")))
}

pub fn read_ethernet_map<P: AsRef<Path>>(path: P) -> Result<EthernetMap, LuwenError> {
    let file = File::open(path).map_err(|e|
        LuwenError::Custom(format!("Failed to open file: {e}")))?;

    serde_yaml::from_reader(file).map_err(|e|
        LuwenError::Custom(format!("Failed to parse YAML: {e}")))
}

#[no_mangle]
/// Creates an ethernet map and writes it to the specified file.
///
/// # Safety
///
/// This function expects a valid, null-terminated C string pointer for the `file` parameter.
/// The caller must ensure that the pointer is valid and properly aligned.
pub unsafe extern "C" fn create_ethernet_map(file: *const std::ffi::c_char) -> std::ffi::c_int {
    if file.is_null() {
        eprintln!("Error file pointer is NULL!");
        return -2;
    }

    let file = std::ffi::CStr::from_ptr(file);
    let filename = file.to_string_lossy();

    match File::create(&*filename) {
        Ok(file) => {
            match generate_map(){
                Ok(map) => {
                    if let Err(value) = write_ethernet_map(file, &map) {
                        eprintln!("Error while writing ethernet map!\n{value}");
                        -1
                    } else {
                        0
                    }
                }
                Err(e) => {
                    eprintln!("Failed to run create ethernet map: {e}");
                    return -1;
                }
            }

        }
        Err(e) => {
            eprintln!("Failed to create file: {e}");
            return -2;
        }
    }
}
