use std::sync::Arc;

use luwen_core::Arch;

use crate::{
    arc_msg::ArcMsgAddr,
    error::{BtWrapper, PlatformError},
    CallbackStorage,
};

use super::{
    chip_comms::load_axi_table, ArcIf, Chip, ChipComms, ChipInterface, Grayskull, Wormhole,
};

impl Chip {
    pub fn gs_open<T: Clone + Send + Sync + 'static>(
        arch: Arch,
        backend: CallbackStorage<T>,
    ) -> Result<Grayskull, PlatformError> {
        if let Arch::Grayskull = arch {
            let mut version = [0u8; 4];
            backend.axi_read(0x0000_0000, &mut version);
            let version = u32::from_le_bytes(version);

            let arc_if = Arc::new(ArcIf {
                axi_data: load_axi_table("grayskull-axi-pci.bin", version),
            });

            Ok(Grayskull {
                chip_if: Arc::new(backend),

                arc_addrs: ArcMsgAddr::try_from(arc_if.as_ref() as &dyn ChipComms)?,

                arc_if,
            })
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
            let mut version = [0u8; 4];
            backend.axi_read(0x0000_0000, &mut version);
            let version = u32::from_le_bytes(version);

            let arc_if = ArcIf {
                axi_data: load_axi_table("wormhole-axi-pci.bin", version),
            };

            Ok(Wormhole::init(
                false,
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

    pub fn open<T: Clone + Send + Sync + 'static>(arch: Arch, backend: CallbackStorage<T>) -> Chip {
        Chip {
            inner: match arch {
                Arch::Grayskull => Box::new(Self::gs_open(arch, backend).unwrap()),
                Arch::Wormhole => Box::new(Self::wh_open(arch, backend).unwrap()),
                _ => panic!("Unsupported chip"),
            },
        }
    }
}
