use std::collections::HashSet;

use crate::{chip::{Chip, wait_for_init}, error::PlatformError, ChipImpl, EthAddr};

#[derive(PartialEq, Eq, Hash, Debug, Clone)]
enum InterfaceIdOrCoord {
    Id(u32),
    Coord(EthAddr),
}

/// Find all chips accessible from the given set of root chips.
/// For the most part this should be a set of chips found via a PCI scan, but it doens't have to be.
///
/// The most important part of this algorithm is determining which chips are duplicates of other chips.
/// In general two boards can be differentiated by their board id, but this is not always the case.
/// For example the gs or wh X2, in that case we must fallback on the interface id for grayskull or ethernet address for wh.
/// However this does not cover all cases, if there is a wh X2 that is not in the root_chips list (which could be because it is in a neighbouring hose)
/// and both chips are in two seperate meshes with the same ethernet address. We will incorrectly detect them as being one chip.
///
/// Search steps:
/// 1. Add all given chips to output list removing duplicates this will ensure that if list indexes are used to
/// assign a chip id pci chips will always be output instead of the remote equivalent.
/// 2. To a depth first search for each root chip, adding all new chips found to the output list.
pub fn detect_chips(mut root_chips: Vec<Chip>, init_callback: &mut impl FnMut(crate::chip::ChipDetectState<'_>)) -> Result<Vec<Chip>, PlatformError> {
    let mut remotes_to_investigate = Vec::new();
    let mut seen_chips = HashSet::new();

    for (root_index, root_chip) in root_chips.iter().enumerate() {
        wait_for_init(root_chip, init_callback)?;

        let ident = if let Some(wh) = root_chip.as_wh() {
            let telem = root_chip.get_telemetry()?;
            remotes_to_investigate.push(root_index);
            (
                Some(telem.board_id),
                Some(InterfaceIdOrCoord::Coord(wh.get_local_chip_coord()?)),
            )
        } else {
            (
                // Can't fetch board id from old gs chips
                // this shouldn't matter anyway because we can only access them
                // via pci
                None,
                root_chip
                    .get_device_info()?
                    .map(|v| InterfaceIdOrCoord::Id(v.interface_id)),
            )
        };

        if !seen_chips.insert(ident) {
            continue;
        }
    }

    let mut output = Vec::new();
    for root_index in remotes_to_investigate {
        let root_chip = &root_chips[root_index];

        let mut to_check = root_chip.get_neighbouring_chips()?;

        let mut seen_coords = HashSet::new();
        while let Some(nchip) = to_check.pop() {
            if !seen_coords.insert(nchip.eth_addr) {
                continue;
            }

            if let Some(wh) = root_chip.as_wh() {
                let wh = wh.open_remote(nchip.eth_addr)?;

                let local_coord = wh.get_local_chip_coord()?;

                if local_coord != nchip.eth_addr {
                    return Err(PlatformError::Generic(
                        format!("When detecting chips in mesh found a mismatch between the expected chip coordinate {} and the actual {}", nchip.eth_addr, local_coord),
                        crate::error::BtWrapper::capture(),
                    ));
                }

                let telem = wh.get_telemetry()?;

                let ident = (
                    Some(telem.board_id),
                    Some(InterfaceIdOrCoord::Coord(local_coord)),
                );

                if !seen_chips.insert(ident) {
                    continue;
                }

                wait_for_init(&wh, init_callback)?;

                for nchip in wh.get_neighbouring_chips()? {
                    if !seen_coords.contains(&nchip.eth_addr) {
                        continue;
                    }

                    to_check.push(nchip);
                }

                output.push(Chip::from(Box::new(wh) as Box<dyn ChipImpl>));
            } else {
                unimplemented!("Don't have a handler for non-WH chips with ethernet support yet.")
            }
        }
    }

    root_chips.extend(output);

    Ok(root_chips)
}

pub fn detect_chips_silent(root_chips: Vec<Chip>) -> Result<Vec<Chip>, PlatformError> {
    detect_chips(root_chips, &mut |_| {})
}
