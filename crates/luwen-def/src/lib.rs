// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt;
use std::str::FromStr;

/// Architecture generation.
///
/// Model specifier for a Tenstorrent architecture generation.
#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum Arch {
    /// Grayskull.
    ///
    /// # Note
    ///
    /// This is a legacy architecture that is no longer supported.
    #[deprecated]
    Grayskull,
    /// Wormhole.
    #[default]
    Wormhole,
    /// Blackhole.
    Blackhole,
}

impl Arch {
    /// Checks if the architecture is [`Arch::Grayskull`].
    #[deprecated]
    #[allow(deprecated)]
    pub fn is_grayskull(&self) -> bool {
        matches!(self, Arch::Grayskull)
    }

    /// Checks if the architecture is [`Arch::Wormhole`].
    pub fn is_wormhole(&self) -> bool {
        matches!(self, Arch::Wormhole)
    }

    /// Checks if the architecture is [`Arch::Blackhole`].
    pub fn is_blackhole(&self) -> bool {
        matches!(self, Arch::Blackhole)
    }
}

impl FromStr for Arch {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            #[allow(deprecated)]
            "grayskull" => Ok(Arch::Grayskull),
            "wormhole" => Ok(Arch::Wormhole),
            "blackhole" => Ok(Arch::Blackhole),
            err => Err(err.to_string()),
        }
    }
}

impl fmt::Display for Arch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
