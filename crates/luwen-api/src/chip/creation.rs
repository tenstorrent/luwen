// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use luwen_def::Arch;

use crate::{
    error::{BtWrapper, PlatformError},
    CallbackStorage,
};

use super::{
    communication::chip_comms::load_axi_table, ArcIf, Blackhole, Chip, ChipComms, ChipInterface,
    Wormhole,
};

impl Chip {
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

    pub fn bh_open<T: Clone + Send + Sync + 'static>(
        arch: Arch,
        backend: CallbackStorage<T>,
    ) -> Result<Blackhole, PlatformError> {
        if let Arch::Blackhole = arch {
            let version = [0u8; 4];
            let version = u32::from_le_bytes(version);

            let arc_if = super::communication::chip_comms::NocIf {
                axi_data: load_axi_table("blackhole-axi-pci.bin", version),
                noc_id: 0,
                x: 8,
                y: 0,
            };

            Ok(Blackhole::init(
                Arc::new(arc_if) as Arc<dyn ChipComms + Sync + Send>,
                Arc::new(super::communication::chip_interface::NocInterface {
                    noc_id: 0,
                    x: 8,
                    y: 0,
                    backing: Box::new(backend),
                }) as Arc<dyn ChipInterface + Sync + Send>,
            )?)
        } else {
            Err(PlatformError::WrongChipArch {
                actual: arch,
                expected: Arch::Blackhole,
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
                #[allow(deprecated)]
                Arch::Grayskull => unimplemented!("grayskull support has been sunset"),
                Arch::Wormhole => Box::new(Self::wh_open(arch, backend)?),
                Arch::Blackhole => Box::new(Self::bh_open(arch, backend)?),
            },
        })
    }
}
