// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

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

    pub fn from_string(s: &str) -> Option<Self> {
        match s {
            "grayskull" => Some(Arch::Grayskull),
            "wormhole" => Some(Arch::Wormhole),
            _ => None,
        }
    }
}
