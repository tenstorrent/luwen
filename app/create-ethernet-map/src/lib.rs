// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, HashSet};

use luwen_core::Arch;
use luwen_if::{
    chip::{ArcMsgOptions, HlComms, NeighbouringChip},
    ChipImpl, EthAddr,
};
use luwen_ref::error::LuwenError;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ChipIdent {
    pub arch: Arch,
    pub board_id: Option<u64>,
    pub interface: Option<u32>,
    pub coord: Option<EthAddr>,
}

#[derive(Debug, Clone)]
pub struct ChipData {
    pub noc_translation_en: bool,
    pub harvest_mask: u32,
    pub boardtype: Option<String>,
}

pub fn generate_map(file: impl AsRef<str>) -> Result<(), LuwenError> {
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
                routing_enabled
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

                connection_info
                    .entry(next_ident)
                    .or_default()
                    .push((local_id, remote_id, routing_enabled));
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

    let mut known_connections = HashSet::new();
    for chip in &ident_order {
        if let Some(connection_info) = connection_map.get(chip) {
            for (remote_chip, connection) in connection_info {
                for (current_eth_id, next_eth_id, routing_enabled) in connection {
                    let local = (chips[chip], current_eth_id);
                    let remote = (chips[remote_chip], next_eth_id);

                    let first = local.min(remote);
                    let second = local.max(remote);

                    let connection_ident = (first, second);
                    let larger_connection_ident = (first, second, routing_enabled);
                    if known_connections.contains(&connection_ident) {
                        continue;
                    }
                    known_connections.insert(connection_ident);

                    connections.push(larger_connection_ident);
                }
            }
        }
    }

    connections.sort();

    let mut output = String::new();

    output.push_str("arch: {\n");
    for chip in &ident_order {
        let id = chips[chip];
        output.push_str(&format!("   {}: {:?},\n", id, chip.arch));
    }
    output.push_str("}\n\n");

    output.push_str("chips: {\n");
    for chip in &ident_order {
        let id = chips[chip];
        if let Some(coord) = &chip.coord {
            output.push_str(&format!(
                "   {}: [{},{},{},{}],\n",
                id, coord.shelf_x, coord.shelf_y, coord.rack_x, coord.rack_y
            ));
        }
    }
    output.push_str("}\n\n");

    output.push_str("ethernet_connections: [\n");
    for ((local_chip, local_port), (remote_chip, remote_port), routing) in connections {
        output.push_str(&format!("   [{{chip: {local_chip}, chan: {local_port}}}, {{chip: {remote_chip}, chan: {remote_port}}}, {{routing_enabled: {routing}}}],\n"));
    }
    output.push_str("]\n\n");

    mmio_chips.sort_by_key(|v| v.1);

    output.push_str("chips_with_mmio: [\n");
    for (mmio, interface) in mmio_chips {
        if let Some(interface) = interface {
            output.push_str(&format!("   {}: {},\n", chips[&mmio], interface));
        }
    }
    output.push_str("]\n\n");

    output.push_str("# harvest_mask is the bit indicating which tensix row is harvested. So bit 0 = first tensix row; bit 1 = second tensix row etc...\n");
    output.push_str("harvesting: {\n");
    for chip in &ident_order {
        let id = chips[chip];
        let data = &chip_data[chip];
        output.push_str(&format!(
            "   {}: {{noc_translation: {}, harvest_mask: {}}},\n",
            id, data.noc_translation_en, data.harvest_mask
        ));
    }
    output.push_str("}\n\n");

    output.push_str("# This value will be null if the boardtype is unknown, should never happen in practice but to be defensive it would be useful to throw an error on this case.\n");
    output.push_str("boardtype: {\n");
    for chip in &ident_order {
        let id = chips[chip];
        let data = &chip_data[chip];
        output.push_str(&format!(
            "   {id}: {},\n",
            data.boardtype.as_deref().unwrap_or("null")
        ));
    }
    output.push('}');

    let file = file.as_ref();

    if let Err(_err) = std::fs::write(file, output) {
        Err(LuwenError::Custom(format!(
            "Failed to write to {}",
            file
        )))
    } else {
        Ok(())
    }
}

#[no_mangle]
pub extern "C" fn create_ethernet_map(file: *const std::ffi::c_char) -> std::ffi::c_int {
    if file.is_null() {
        eprintln!("Error file pointer is NULL!");
        return -2;
    }

    let file = unsafe { std::ffi::CStr::from_ptr(file) };
    if let Err(value) = generate_map(file.to_string_lossy()) {
        eprintln!("Error while generating ethernet map!\n{value}");
        -1
    } else {
        0
    }
}
