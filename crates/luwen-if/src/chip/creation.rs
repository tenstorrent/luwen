// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use luwen_core::Arch;

use crate::{
    arc_msg::ArcMsgAddr,
    error::{BtWrapper, PlatformError},
    CallbackStorage,
};

use super::{
    communication::chip_comms::load_axi_table, ArcIf, Chip, ChipComms, ChipInterface, Grayskull,
    Wormhole,
};

impl Chip {
    pub fn gs_open<T: Clone + Send + Sync + 'static>(
        arch: Arch,
        backend: CallbackStorage<T>,
    ) -> Result<Grayskull, PlatformError> {
        if let Arch::Grayskull = arch {
            let version = [0u8; 4];
            let version = u32::from_le_bytes(version);

            let arc_if = Arc::new(ArcIf {
                axi_data: load_axi_table("grayskull-axi-pci.bin", version),
            });

            Ok(Grayskull::create(
                Arc::new(backend),
                arc_if.clone(),
                ArcMsgAddr::try_from(arc_if.as_ref() as &dyn ChipComms)?,
            ))
        } else {
            Err(PlatformError::WrongChipArch {
                actual: arch,
                expected: Arch::Grayskull,
                backtrace: BtWrapper::capture(),
            })
        }
    }

    pub fn wh_open<T: Clone + Send + Sync + 'static>(
        arch: Arch,
        backend: CallbackStorage<T>,
    ) -> Result<Wormhole, PlatformError> {
        if let Arch::Wormhole = arch {
            let version = [0u8; 4];
            let version = u32::from_le_bytes(version);

            let arc_if = ArcIf {
                axi_data: load_axi_table("wormhole-axi-pci.bin", version),
            };

            Ok(Wormhole::init(
                false,
                true,
                arc_if,
                Arc::new(backend) as Arc<dyn ChipInterface + Sync + Send>,
            )?)
        } else {
            Err(PlatformError::WrongChipArch {
                actual: arch,
                expected: Arch::Wormhole,
                backtrace: BtWrapper::capture(),
            })
        }
    }

    pub fn open<T: Clone + Send + Sync + 'static>(
        arch: Arch,
        backend: CallbackStorage<T>,
    ) -> Result<Chip, PlatformError> {
        Ok(Chip {
            inner: match arch {
                Arch::Grayskull => Box::new(Self::gs_open(arch, backend)?),
                Arch::Wormhole => Box::new(Self::wh_open(arch, backend)?),
                _ => panic!("Unsupported chip"),
            },
        })
    }
}
