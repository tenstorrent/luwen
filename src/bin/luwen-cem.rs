// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, HashSet};

use clap::Parser;

use luwen_core::Arch;
use luwen_if::{
    chip::{ArcMsgOptions, HlComms, NeighbouringChip},
    ChipImpl, EthAddr,
};
use luwen_ref::error::LuwenError;

#[derive(Parser)]
pub struct CmdArgs {
    file: String,
}

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
}

fn main() -> Result<(), LuwenError> {
    let args = CmdArgs::parse();

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
                    msg: luwen_if::ArcMsg::GetHarvesting,
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

                    let local_id = wh
                        .eth_locations
                        .iter()
                        .position(|v| (v.x, v.y) == local_noc_addr)
                        .unwrap();

                    let remote_id = wh
                        .eth_locations
                        .iter()
                        .position(|v| (v.x, v.y) == remote_noc_addr)
                        .unwrap();

                    connection_info
                        .entry(next_ident)
                        .or_default()
                        .push((local_id, remote_id));
                }
                connection_map.insert(ident.clone(), connection_info);
            }

            (ident, data)
        } else if let Some(gs) = chip.as_gs() {
            let result = gs
                .arc_msg(ArcMsgOptions {
                    msg: luwen_if::ArcMsg::GetHarvesting,
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
            };

            mmio_chips.push((ident.clone(), gs.get_device_info()?.map(|v| v.interface_id)));

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
        ident_order.push(chip.clone());
    }
    ident_order.sort_by_key(|v| v.1);
    let ident_order: Vec<_> = ident_order.into_iter().map(|v| v.0.clone()).collect();

    let mut known_connections = HashSet::new();
    for chip in &ident_order {
        if let Some(connection_info) = connection_map.get(&chip) {
            for (remote_chip, connection) in connection_info {
                for (current_eth_id, next_eth_id) in connection {
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
            output.push_str(&format!("   {}: {},\n", id, coord));
        }
    }
    output.push_str("}\n\n");

    output.push_str("ethernet_connections: [\n");
    for ((local_chip, local_port), (remote_chip, remote_port)) in connections {
        output.push_str(&format!("   [{{chip: {local_chip}, chan: {local_port}}}, {{chip: {remote_chip}, chan: {remote_port}}}],\n"));
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

    output.push_str("harvesting: [\n");
    for chip in &ident_order {
        let id = chips[chip];
        let data = &chip_data[chip];
        output.push_str(&format!(
            "   {}: {{noc_translation: {}, harvest_mask: {}}},\n",
            id, data.noc_translation_en, data.harvest_mask
        ));
    }
    output.push_str("]");

    if let Err(_err) = std::fs::write(&args.file, output) {
        Err(LuwenError::Custom(format!(
            "Failed to write to {}",
            args.file
        )))
    } else {
        Ok(())
    }
}
