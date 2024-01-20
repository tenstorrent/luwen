// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

#[derive(Clone, Hash, Copy, Debug, PartialEq, Eq)]
pub enum Arch {
    Grayskull,
    Wormhole,
    Unknown(u16),
}

impl Arch {
    pub fn is_wormhole(&self) -> bool {
        match self {
            Arch::Wormhole => true,
            _ => false,
        }
    }

    pub fn is_grayskull(&self) -> bool {
        match self {
            Arch::Grayskull => true,
            _ => false,
        }
    }
}

impl FromStr for Arch {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "grayskull" => Ok(Arch::Grayskull),
            "wormhole" => Ok(Arch::Wormhole),
            err => Err(err.to_string()),
        }
    }
}
