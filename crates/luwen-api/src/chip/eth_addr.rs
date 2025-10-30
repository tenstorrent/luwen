// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt::Display;

use crate::error::PlatformError;

use super::{ChipComms, ChipInterface};

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct EthAddr {
    pub shelf_x: u8,
    pub shelf_y: u8,
    pub rack_x: u8,
    pub rack_y: u8,
}

pub trait IntoChip<T>: Sized {
    fn cinto(&self, chip: &dyn ChipComms, cif: &dyn ChipInterface) -> Result<T, PlatformError>;
}

pub fn get_local_chip_coord(
    chip: &dyn ChipComms,
    cif: &dyn ChipInterface,
) -> Result<EthAddr, PlatformError> {
    let mut coord = [0; 4];
    chip.noc_read(cif, 0, 9, 0, 0x1108, &mut coord)?;
    let coord = u32::from_le_bytes(coord);

    Ok(EthAddr {
        rack_x: (coord & 0xFF) as u8,
        rack_y: ((coord >> 8) & 0xFF) as u8,
        shelf_x: ((coord >> 16) & 0xFF) as u8,
        shelf_y: ((coord >> 24) & 0xFF) as u8,
    })
}

impl IntoChip<EthAddr> for EthAddr {
    fn cinto(
        &self,
        _chip: &dyn ChipComms,
        _cif: &dyn ChipInterface,
    ) -> Result<EthAddr, PlatformError> {
        Ok(*self)
    }
}

impl IntoChip<EthAddr> for (Option<u8>, Option<u8>, Option<u8>, Option<u8>) {
    fn cinto(
        &self,
        chip: &dyn ChipComms,
        cif: &dyn ChipInterface,
    ) -> Result<EthAddr, PlatformError> {
        let local_coord = get_local_chip_coord(chip, cif)?;

        let rack_x = self.0.unwrap_or(local_coord.rack_x);
        let rack_y = self.1.unwrap_or(local_coord.rack_y);
        let shelf_x = self.2.unwrap_or(local_coord.shelf_x);
        let shelf_y = self.3.unwrap_or(local_coord.shelf_y);

        Ok(EthAddr {
            rack_x,
            rack_y,
            shelf_x,
            shelf_y,
        })
    }
}

impl IntoChip<EthAddr> for (u8, u8, u8, u8) {
    fn cinto(
        &self,
        _chip: &dyn ChipComms,
        _cif: &dyn ChipInterface,
    ) -> Result<EthAddr, PlatformError> {
        let (rack_x, rack_y, shelf_x, shelf_y) = *self;

        Ok(EthAddr {
            rack_x,
            rack_y,
            shelf_x,
            shelf_y,
        })
    }
}

impl IntoChip<EthAddr> for (u8, u8) {
    fn cinto(
        &self,
        chip: &dyn ChipComms,
        cif: &dyn ChipInterface,
    ) -> Result<EthAddr, PlatformError> {
        (None, None, Some(self.0), Some(self.1)).cinto(chip, cif)
    }
}

impl Display for EthAddr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "[{rack_x}, {rack_y}, {shelf_x}, {self_y}]",
            rack_x = self.rack_x,
            rack_y = self.rack_y,
            shelf_x = self.shelf_x,
            self_y = self.shelf_y
        ))
    }
}
