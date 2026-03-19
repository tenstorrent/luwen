// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{backtrace, collections::HashMap, sync::Arc};

use num_traits::cast::FromPrimitive;

use crate::{
    arc_msg::{ArcMsgAddr, ArcMsgOk, TypedArcMsg},
    chip::{
        communication::{
            chip_comms::{load_axi_table, ChipComms},
            chip_interface::ChipInterface,
        },
        hl_comms::HlCommsInterface,
    },
    error::{BtWrapper, PlatformError},
    ArcMsg, ChipImpl, IntoChip,
};

use super::{
    blackhole::telemetry_tags::TelemetryTags,
    eth_addr::EthAddr,
    hl_comms::HlComms,
    init::status::{ComponentStatusInfo, EthernetPartialInitError, InitOptions, WaitStatus},
    remote::{EthAddresses, RemoteArcIf},
    ArcMsgOptions, ChipInitResult, CommsStatus, InitStatus, NeighbouringChip,
};

/// Cached state for the new (BH-style) telemetry table.
/// The tag-to-offset mapping is stable across calls; only the data values change.
#[derive(Clone)]
struct WormholeNewTelemetry {
    /// AXI address of the `telemetry_data` array (host-accessible).
    telemetry_data_axi: u64,
    /// Maps each telemetry tag value to its index in `telemetry_data`.
    tag_offsets: HashMap<u16, u16>,
}

/// Implementation of the interface for a Wormhole
/// both the local and remote Wormhole chips are represented by this struct
#[derive(Clone)]
pub struct Wormhole {
    pub chip_if: Arc<dyn ChipInterface + Send + Sync>,
    pub arc_if: Arc<dyn ChipComms + Send + Sync>,

    pub is_remote: bool,
    pub use_arc_for_spi: bool,

    pub arc_addrs: ArcMsgAddr,
    pub eth_locations: [EthCore; 16],
    pub eth_addrs: EthAddresses,
    /// Cached address for the legacy SMBUS telemetry (fallback for old firmware).
    telemetry_addr: Arc<once_cell::sync::OnceCell<u32>>,
    /// Cached new (BH-style) telemetry table info. `None` means new telemetry is not
    /// available (old firmware); `Some(...)` contains the stable tag-to-offset map.
    new_telemetry: Arc<once_cell::sync::OnceCell<Option<WormholeNewTelemetry>>>,
}

impl HlComms for Wormhole {
    fn comms_obj(&self) -> (&dyn ChipComms, &dyn ChipInterface) {
        (self.arc_if.as_ref(), self.chip_if.as_ref())
    }
}

impl HlComms for &Wormhole {
    fn comms_obj(&self) -> (&dyn ChipComms, &dyn ChipInterface) {
        (self.arc_if.as_ref(), self.chip_if.as_ref())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct EthCore {
    pub x: u8,
    pub y: u8,
    pub enabled: bool,
}

impl Default for EthCore {
    fn default() -> Self {
        Self {
            x: 0,
            y: 0,
            enabled: true,
        }
    }
}

/// Convert the new TAG_GDDR_STATUS encoding (2 bits per tile: bit 0 = training
/// success, bit 1 = error) into the legacy `ddr_status` encoding used by
/// `update_init_state` (4 bits per channel matching `DramChannelStatus`:
/// 0 = None, 1 = Fail, 2 = Pass).
///
/// WH has six GDDR tiles (0–5); bits for tiles 6–7 are ignored.
fn new_gddr_status_to_ddr_status(new_status: u32) -> u32 {
    let mut ddr_status = 0u32;
    for i in 0..6usize {
        let success = (new_status >> (i * 2)) & 1;
        let error = (new_status >> (i * 2 + 1)) & 1;
        let channel_val = if error != 0 {
            1u32 // TrainingFail
        } else if success != 0 {
            2u32 // TrainingPass
        } else {
            0u32 // TrainingNone
        };
        ddr_status |= channel_val << (i * 4);
    }
    ddr_status
}

impl Wormhole {
    pub(crate) fn init<
        CC: ChipComms + Send + Sync + 'static,
        CI: ChipInterface + Send + Sync + 'static,
    >(
        is_remote: bool,
        use_arc_for_spi: bool,
        arc_if: CC,
        chip_if: CI,
    ) -> Result<Self, PlatformError> {
        // let mut version = [0; 4];
        // arc_if.axi_read(&chip_if, 0x0, &mut version);
        // let version = u32::from_le_bytes(version);
        let _version = 0x0;

        let output = Wormhole {
            chip_if: Arc::new(chip_if),

            is_remote,
            use_arc_for_spi,

            arc_addrs: ArcMsgAddr {
                scratch_base: arc_if.axi_translate("ARC_RESET.SCRATCH[0]")?.addr,
                arc_misc_cntl: arc_if.axi_translate("ARC_RESET.ARC_MISC_CNTL")?.addr,
            },

            arc_if: Arc::new(arc_if),
            eth_addrs: EthAddresses::default(),

            telemetry_addr: Arc::new(once_cell::sync::OnceCell::new()),
            new_telemetry: Arc::new(once_cell::sync::OnceCell::new()),

            eth_locations: [
                EthCore {
                    x: 9,
                    y: 0,
                    ..Default::default()
                },
                EthCore {
                    x: 1,
                    y: 0,
                    ..Default::default()
                },
                EthCore {
                    x: 8,
                    y: 0,
                    ..Default::default()
                },
                EthCore {
                    x: 2,
                    y: 0,
                    ..Default::default()
                },
                EthCore {
                    x: 7,
                    y: 0,
                    ..Default::default()
                },
                EthCore {
                    x: 3,
                    y: 0,
                    ..Default::default()
                },
                EthCore {
                    x: 6,
                    y: 0,
                    ..Default::default()
                },
                EthCore {
                    x: 4,
                    y: 0,
                    ..Default::default()
                },
                EthCore {
                    x: 9,
                    y: 6,
                    ..Default::default()
                },
                EthCore {
                    x: 1,
                    y: 6,
                    ..Default::default()
                },
                EthCore {
                    x: 8,
                    y: 6,
                    ..Default::default()
                },
                EthCore {
                    x: 2,
                    y: 6,
                    ..Default::default()
                },
                EthCore {
                    x: 7,
                    y: 6,
                    ..Default::default()
                },
                EthCore {
                    x: 3,
                    y: 6,
                    ..Default::default()
                },
                EthCore {
                    x: 6,
                    y: 6,
                    ..Default::default()
                },
                EthCore {
                    x: 4,
                    y: 6,
                    ..Default::default()
                },
            ],
        };

        Ok(output)
    }

    pub fn init_eth_addrs(&mut self) -> Result<(), PlatformError> {
        if self.eth_addrs.masked_version == 0 {
            let telemetry = self.get_telemetry()?;

            self.eth_addrs = EthAddresses::new(telemetry.eth_fw_version);
        }

        Ok(())
    }

    pub fn get_if<T: ChipInterface>(&self) -> Option<&T> {
        self.chip_if.as_any().downcast_ref::<T>()
    }

    pub fn open_remote(&self, addr: impl IntoChip<EthAddr>) -> Result<Wormhole, PlatformError> {
        let arc_if = RemoteArcIf {
            addr: addr.cinto(&self.arc_if, &self.chip_if).unwrap(),
            axi_data: Some(load_axi_table("wormhole-axi-noc.bin", 0)),
        };

        Self::init(true, true, arc_if, self.chip_if.clone())
    }

    // fn check_dram_trained(&self) {
    //     let pc = self.axi_sread32("ARC_RESET.POST_CODE")?;

    //     0x29
    // }

    /// Attempt to initialise the new (BH-style) telemetry table available since
    /// firmware bundle ≥ 18.4.
    ///
    /// The ARC Reset Unit publishes two addresses in its `NOC_NODEID_X_0` and
    /// `NOC_NODEID_Y_0` registers (at Reset Unit offsets 0x01D0 / 0x01D4):
    ///   * `NOC_NODEID_X_0` – ARC CPU address of the immutable `telemetry_table`
    ///     (version + entry_count + tag_table[]).
    ///   * `NOC_NODEID_Y_0` – ARC CPU address of the mutable `telemetry_data`
    ///     array (one u32 per entry, indexed by the offsets in tag_table).
    ///
    /// Returns `None` when `NOC_NODEID_X_0` is zero (old firmware that does not
    /// support new-style telemetry).
    fn try_init_new_telemetry(&self) -> Result<Option<WormholeNewTelemetry>, PlatformError> {
        // SCRATCH[0] sits at Reset Unit offset 0x0060.  NOC_NODEID_X_0 / Y_0
        // are at Reset Unit offsets 0x01D0 / 0x01D4.  Derive their AXI
        // addresses from SCRATCH[0] so that the same calculation works for
        // both PCI (local) and NOC (remote) access patterns.
        // NOC_NODEID_X_0 - SCRATCH[0] = 0x01D0 - 0x0060 = 0x0170
        // NOC_NODEID_Y_0 - SCRATCH[0] = 0x01D4 - 0x0060 = 0x0174
        let scratch0_addr = self.arc_if.axi_translate("ARC_RESET.SCRATCH[0]")?.addr;
        let noc_nodeid_x0 = scratch0_addr + 0x0170;
        let noc_nodeid_y0 = scratch0_addr + 0x0174;

        let telem_table_arc_addr = self.arc_if.axi_read32(&self.chip_if, noc_nodeid_x0)?;

        // A zero value means the firmware does not publish new-style telemetry.
        if telem_table_arc_addr == 0 {
            return Ok(None);
        }

        let telem_data_arc_addr = self.arc_if.axi_read32(&self.chip_if, noc_nodeid_y0)?;

        // Both addresses must be within the CSM region (0x10000000 – 0x1007FFFF).
        if !(0x10000000..=0x1007FFFF).contains(&telem_table_arc_addr) {
            return Err(PlatformError::Generic(
                format!(
                    "Invalid telemetry table address: 0x{telem_table_arc_addr:08x}"
                ),
                BtWrapper::capture(),
            ));
        }
        if !(0x10000000..=0x1007FFFF).contains(&telem_data_arc_addr) {
            return Err(PlatformError::Generic(
                format!(
                    "Invalid telemetry data address: 0x{telem_data_arc_addr:08x}"
                ),
                BtWrapper::capture(),
            ));
        }

        // Convert ARC CPU addresses to host-visible AXI addresses.
        // ARC CSM starts at CPU address 0x10000000; ARC_CSM.DATA[0] gives
        // the corresponding AXI base.
        let csm_offset = self.arc_if.axi_translate("ARC_CSM.DATA[0]")?;
        let telem_table_axi =
            csm_offset.addr + (telem_table_arc_addr - 0x10000000) as u64;
        let telem_data_axi =
            csm_offset.addr + (telem_data_arc_addr - 0x10000000) as u64;

        // Read the telemetry table header.
        // Layout: [version: u32][entry_count: u32][tag_table: entry_count * u32]
        // Each tag_table entry: low 16 bits = tag, high 16 bits = index into telemetry_data.
        let entry_count = self.arc_if.axi_read32(&self.chip_if, telem_table_axi + 4)?;

        // Sanity-check entry_count before allocating.  The firmware currently
        // defines fewer than 70 tags; guard against corrupted data.
        const MAX_TELEM_ENTRIES: u32 = 256;
        if entry_count > MAX_TELEM_ENTRIES {
            return Err(PlatformError::Generic(
                format!("Telemetry entry_count too large: {entry_count}"),
                BtWrapper::capture(),
            ));
        }

        // Build the stable tag → index mapping.
        let mut tag_offsets = HashMap::with_capacity(entry_count as usize);
        for i in 0..entry_count {
            let entry_axi = telem_table_axi + 8 + i as u64 * 4;
            let entry = self.arc_if.axi_read32(&self.chip_if, entry_axi)?;
            let tag = (entry & 0xFFFF) as u16;
            let offset = ((entry >> 16) & 0xFFFF) as u16;
            tag_offsets.insert(tag, offset);
        }

        Ok(Some(WormholeNewTelemetry {
            telemetry_data_axi: telem_data_axi,
            tag_offsets,
        }))
    }

    /// Read telemetry using the new (BH-style) tag-based interface.
    fn get_telemetry_new(
        &self,
        new_telem: &WormholeNewTelemetry,
    ) -> Result<super::Telemetry, PlatformError> {
        // Read the entire telemetry_data array in a single bulk AXI transfer to
        // minimise the number of hardware round-trips.
        let max_offset = new_telem
            .tag_offsets
            .values()
            .copied()
            .max()
            .unwrap_or(0) as usize;
        let data_len = (max_offset + 1) * 4;
        let mut data_buf = vec![0u8; data_len];
        self.arc_if
            .axi_read(&self.chip_if, new_telem.telemetry_data_axi, &mut data_buf)?;

        let mut t = super::Telemetry::default();

        for (&tag, &offset) in &new_telem.tag_offsets {
            let idx = offset as usize * 4;
            if idx + 4 > data_buf.len() {
                continue;
            }
            let data = u32::from_le_bytes([
                data_buf[idx],
                data_buf[idx + 1],
                data_buf[idx + 2],
                data_buf[idx + 3],
            ]);
            let Some(tag) = TelemetryTags::from_u32(tag as u32) else {
                continue;
            };
            match tag {
                TelemetryTags::BoardIdHigh => t.board_id_high = data,
                TelemetryTags::BoardIdLow => t.board_id_low = data,
                TelemetryTags::AsicId => t.asic_id = data,
                TelemetryTags::HarvestingState => t.harvesting_state = data,
                TelemetryTags::UpdateTelemSpeed => t.update_telem_speed = data,
                TelemetryTags::Vcore => t.vcore = data,
                TelemetryTags::Tdp => t.tdp = data,
                TelemetryTags::Tdc => t.tdc = data,
                TelemetryTags::VddLimits => t.vdd_limits = data,
                TelemetryTags::ThmLimits => t.thm_limits = data,
                TelemetryTags::AsicTemperature => t.asic_temperature = data,
                TelemetryTags::VregTemperature => t.vreg_temperature = data,
                TelemetryTags::BoardTemperature => t.board_temperature = data,
                TelemetryTags::AiClk => t.aiclk = data,
                TelemetryTags::AxiClk => t.axiclk = data,
                TelemetryTags::ArcClk => t.arcclk = data,
                TelemetryTags::EthLiveStatus => t.eth_status0 = data,
                TelemetryTags::DdrStatus => {
                    // The new TAG_GDDR_STATUS uses 2 bits per tile (bit 0 = training
                    // success, bit 1 = error).  The legacy ddr_status field and the
                    // update_init_state parser expect 4 bits per tile matching the
                    // DramChannelStatus enum (0=None, 1=Fail, 2=Pass).  Convert here.
                    t.ddr_status = new_gddr_status_to_ddr_status(data);
                }
                TelemetryTags::DdrSpeed => t.ddr_speed = Some(data),
                TelemetryTags::EthFwVersion => t.eth_fw_version = data,
                TelemetryTags::DdrFwVersion => t.ddr_fw_version = data,
                TelemetryTags::BmAppFwVersion => t.m3_app_fw_version = data,
                TelemetryTags::BmBlFwVersion => t.m3_bl_fw_version = data,
                TelemetryTags::FlashBundleVersion => t.fw_bundle_version = data,
                TelemetryTags::CmFwVersion => t.arc0_fw_version = data,
                TelemetryTags::FanSpeed => t.fan_speed = data,
                TelemetryTags::TimerHeartbeat => {
                    t.timer_heartbeat = data;
                    // Maintain backward compatibility: arc0_health mirrors the heartbeat.
                    t.arc0_health = data;
                }
                TelemetryTags::NocTranslation => t.noc_translation_enabled = data != 0,
                TelemetryTags::FwBuildDate => t.wh_fw_date = data,
                TelemetryTags::TtFlashVersion => t.tt_flash_version = data,
                TelemetryTags::AsicLocation => t.asic_location = data,
                TelemetryTags::BoardPowerLimit => t.board_power_limit = data,
                TelemetryTags::InputPower => t.input_power = data,
                TelemetryTags::TdcLimitMax => t.tdc_limit_max = data,
                TelemetryTags::ThmLimitThrottle => t.thm_limit_throttle = data,
                TelemetryTags::ThermTripCount => t.therm_trip_count = data,
                TelemetryTags::AsicIdHigh => t.asic_id_high = data,
                TelemetryTags::AsicIdLow => t.asic_id_low = data,
                TelemetryTags::AiclkLimitMax => t.aiclk_limit_max = data,
                TelemetryTags::TdpLimitMax => t.tdp_limit_max = data,
                _ => {}
            }
        }

        t.board_id = ((t.board_id_high as u64) << 32) | t.board_id_low as u64;
        t.arch = self.get_arch();
        Ok(t)
    }

    /// Read telemetry using the legacy SMBUS telemetry interface (old firmware).
    fn get_telemetry_legacy(&self) -> Result<super::Telemetry, PlatformError> {
        // Field indices in the legacy SMBUS telemetry struct.
        // These are fixed by the firmware ABI and are different from the
        // BH-style TelemetryTags values.
        const ENUM_VERSION: u64 = 0;
        const DEVICE_ID: u64 = 1;
        const ASIC_RO: u64 = 2;
        const ASIC_IDD: u64 = 3;
        const BOARD_ID_HIGH: u64 = 4;
        const BOARD_ID_LOW: u64 = 5;
        const ARC0_FW_VERSION: u64 = 6;
        const ARC1_FW_VERSION: u64 = 7;
        const ARC2_FW_VERSION: u64 = 8;
        const ARC3_FW_VERSION: u64 = 9;
        const SPIBOOTROM_FW_VERSION: u64 = 10;
        const ETH_FW_VERSION: u64 = 11;
        const M3_BL_FW_VERSION: u64 = 12;
        const M3_APP_FW_VERSION: u64 = 13;
        const DDR_STATUS: u64 = 14;
        const ETH_STATUS0: u64 = 15;
        const ETH_STATUS1: u64 = 16;
        const PCIE_STATUS: u64 = 17;
        const FAULTS: u64 = 18;
        const ARC0_HEALTH: u64 = 19;
        const ARC1_HEALTH: u64 = 20;
        const ARC2_HEALTH: u64 = 21;
        const ARC3_HEALTH: u64 = 22;
        const FAN_SPEED: u64 = 23;
        const AICLK: u64 = 24;
        const AXICLK: u64 = 25;
        const ARCCLK: u64 = 26;
        const THROTTLER: u64 = 27;
        const VCORE: u64 = 28;
        const ASIC_TEMPERATURE: u64 = 29;
        const VREG_TEMPERATURE: u64 = 30;
        const BOARD_TEMPERATURE: u64 = 31;
        const TDP: u64 = 32;
        const TDC: u64 = 33;
        const VDD_LIMITS: u64 = 34;
        const THM_LIMITS: u64 = 35;
        const WH_FW_DATE: u64 = 36;
        const ASIC_TMON0: u64 = 37;
        const ASIC_TMON1: u64 = 38;
        const MVDDQ_POWER: u64 = 39;
        const GDDR_TRAIN_TEMP0: u64 = 40;
        const GDDR_TRAIN_TEMP1: u64 = 41;
        const BOOT_DATE: u64 = 42;
        const RT_SECONDS: u64 = 43;
        const ETH_DEBUG_STATUS0: u64 = 44;
        const ETH_DEBUG_STATUS1: u64 = 45;
        const TT_FLASH_VERSION: u64 = 46;
        const FW_BUNDLE_VERSION: u64 = 49;

        let offset: Result<u32, PlatformError> = self
            .telemetry_addr
            .get_or_try_init(|| {
                let result = self.arc_msg(ArcMsgOptions {
                    msg: ArcMsg::Typed(TypedArcMsg::GetSmbusTelemetryAddr),
                    ..Default::default()
                })?;

                let offset = match result {
                    ArcMsgOk::Ok { arg, .. } => arg,
                    ArcMsgOk::OkBuf([_, arg, ..]) => arg,
                    ArcMsgOk::OkNoWait => todo!(),
                };

                Ok(offset)
            })
            .copied();

        let offset = offset?;

        let csm_offset = self.arc_if.axi_translate("ARC_CSM.DATA[0]")?;

        let telemetry_struct_offset = csm_offset.addr + (offset - 0x10000000) as u64;
        let enum_version = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + ENUM_VERSION * 4)?;
        let device_id = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + DEVICE_ID * 4)?;
        let asic_ro = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + ASIC_RO * 4)?;
        let asic_idd = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + ASIC_IDD * 4)?;

        let board_id_high = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + BOARD_ID_HIGH * 4)?;
        let board_id_low = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + BOARD_ID_LOW * 4)?;
        let arc0_fw_version = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + ARC0_FW_VERSION * 4)?;
        let arc1_fw_version = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + ARC1_FW_VERSION * 4)?;
        let arc2_fw_version = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + ARC2_FW_VERSION * 4)?;
        let arc3_fw_version = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + ARC3_FW_VERSION * 4)?;
        let spibootrom_fw_version = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + SPIBOOTROM_FW_VERSION * 4)?;
        let eth_fw_version = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + ETH_FW_VERSION * 4)?;
        let m3_bl_fw_version = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + M3_BL_FW_VERSION * 4)?;
        let m3_app_fw_version = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + M3_APP_FW_VERSION * 4)?;
        let ddr_status = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + DDR_STATUS * 4)?;
        let eth_status0 = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + ETH_STATUS0 * 4)?;
        let eth_status1 = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + ETH_STATUS1 * 4)?;
        let pcie_status = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + PCIE_STATUS * 4)?;
        let faults = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + FAULTS * 4)?;
        let arc0_health = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + ARC0_HEALTH * 4)?;
        let arc1_health = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + ARC1_HEALTH * 4)?;
        let arc2_health = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + ARC2_HEALTH * 4)?;
        let arc3_health = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + ARC3_HEALTH * 4)?;
        let fan_speed = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + FAN_SPEED * 4)?;
        let aiclk = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + AICLK * 4)?;
        let axiclk = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + AXICLK * 4)?;
        let arcclk = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + ARCCLK * 4)?;
        let throttler = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + THROTTLER * 4)?;
        let vcore = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + VCORE * 4)?;
        let asic_temperature = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + ASIC_TEMPERATURE * 4)?;
        let vreg_temperature = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + VREG_TEMPERATURE * 4)?;
        let board_temperature = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + BOARD_TEMPERATURE * 4)?;
        let tdp = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + TDP * 4)?;
        let tdc = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + TDC * 4)?;
        let vdd_limits = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + VDD_LIMITS * 4)?;
        let thm_limits = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + THM_LIMITS * 4)?;
        let wh_fw_date = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + WH_FW_DATE * 4)?;
        let asic_tmon0 = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + ASIC_TMON0 * 4)?;
        let asic_tmon1 = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + ASIC_TMON1 * 4)?;
        let mvddq_power = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + MVDDQ_POWER * 4)?;
        let gddr_train_temp0 = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + GDDR_TRAIN_TEMP0 * 4)?;
        let gddr_train_temp1 = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + GDDR_TRAIN_TEMP1 * 4)?;
        let boot_date = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + BOOT_DATE * 4)?;
        let rt_seconds = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + RT_SECONDS * 4)?;
        let eth_debug_status0 = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + ETH_DEBUG_STATUS0 * 4)?;
        let eth_debug_status1 = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + ETH_DEBUG_STATUS1 * 4)?;
        let tt_flash_version = self
            .arc_if
            .axi_read32(&self.chip_if, telemetry_struct_offset + TT_FLASH_VERSION * 4)?;

        let threshold: u32 = 0x02190000; // arc fw 2.25.0.0
        let fw_bundle_version: u32 = if arc0_fw_version >= threshold {
            self.arc_if
                .axi_read32(&self.chip_if, telemetry_struct_offset + FW_BUNDLE_VERSION * 4)?
        } else {
            0
        };

        Ok(super::Telemetry {
            arch: self.get_arch(),
            board_id: ((board_id_high as u64) << 32) | (board_id_low as u64),
            enum_version,
            device_id,
            asic_ro,
            asic_idd,
            board_id_high,
            board_id_low,
            arc0_fw_version,
            arc1_fw_version,
            arc2_fw_version,
            arc3_fw_version,
            spibootrom_fw_version,
            eth_fw_version,
            m3_bl_fw_version,
            m3_app_fw_version,
            ddr_status,
            eth_status0,
            eth_status1,
            pcie_status,
            faults,
            arc0_health,
            arc1_health,
            arc2_health,
            arc3_health,
            fan_speed,
            aiclk,
            axiclk,
            arcclk,
            throttler,
            vcore,
            asic_temperature,
            vreg_temperature,
            board_temperature,
            tdp,
            tdc,
            vdd_limits,
            thm_limits,
            wh_fw_date,
            asic_tmon0,
            asic_tmon1,
            mvddq_power,
            gddr_train_temp0,
            gddr_train_temp1,
            boot_date,
            rt_seconds,
            eth_debug_status0,
            eth_debug_status1,
            tt_flash_version,
            fw_bundle_version,
            timer_heartbeat: arc0_health,
            ..Default::default()
        })
    }

    fn check_arc_msg_safe(&self, msg_reg: u64, _return_reg: u64) -> Result<(), PlatformError> {
        const POST_CODE_INIT_DONE: u32 = 0xC0DE0001;
        const _POST_CODE_ARC_MSG_HANDLE_START: u32 = 0xC0DE0030;
        const POST_CODE_ARC_MSG_HANDLE_DONE: u32 = 0xC0DE003F;
        const POST_CODE_ARC_TIME_LAST: u32 = 0xC0DE007F;

        let s5 = self.axi_sread32(format!("ARC_RESET.SCRATCH[{msg_reg}]"))?;
        let pc = self.axi_sread32("ARC_RESET.POST_CODE")?;
        let dma = self.axi_sread32("ARC_CSM.ARC_PCIE_DMA_REQUEST.trigger")?;

        if pc == 0xFFFFFFFF {
            return Err(PlatformError::ArcNotReady(
                crate::error::ArcReadyError::NoAccess,
                BtWrapper::capture(),
            ))?;
        }

        if s5 == 0xDEADC0DE {
            return Err(PlatformError::ArcNotReady(
                crate::error::ArcReadyError::WatchdogTriggered,
                BtWrapper::capture(),
            ))?;
        }

        // Still booting and it will later wipe SCRATCH[5/2].
        if s5 == 0x00000060 || pc == 0x11110000 {
            return Err(PlatformError::ArcNotReady(
                crate::error::ArcReadyError::BootIncomplete,
                BtWrapper::capture(),
            ))?;
        }

        if s5 == 0x0000AA00 || s5 == TypedArcMsg::ArcGoToSleep.msg_code() as u32 {
            return Err(PlatformError::ArcNotReady(
                crate::error::ArcReadyError::Asleep,
                BtWrapper::capture(),
            ))?;
        }

        // PCIE DMA writes SCRATCH[5] on exit, so it's not safe.
        // Also we assume FW is hung if we see this state.
        // (The former is only relevant when msg_reg==5, but the latter is always relevant.)
        if dma != 0 {
            return Err(PlatformError::ArcNotReady(
                crate::error::ArcReadyError::OutstandingPcieDMA,
                BtWrapper::capture(),
            ))?;
        }

        if s5 & 0xFFFFFF00 == 0x0000AA00 {
            let message_id = s5 & 0xFF;
            return Err(PlatformError::ArcNotReady(
                crate::error::ArcReadyError::MessageQueued(message_id),
                BtWrapper::capture(),
            ))?;
        }

        if s5 & 0xFF00FFFF == 0xAA000000 {
            let message_id = (s5 >> 16) & 0xFF;
            return Err(PlatformError::ArcNotReady(
                crate::error::ArcReadyError::HandlingMessage(message_id),
                BtWrapper::capture(),
            ))?;
        }

        // Boot complete (new FW only), message not recognized,
        // pcie_dma_{chip_to_host,host_to_chip}_transfer failed to acquire PCIE mutex
        if let 0x00000001 | 0xFFFFFFFF | 0xFFFFDEAD = s5 {
            return Ok(());
        }

        if s5 & 0x0000FFFF > 0x00000001 {
            // YYYY00XX for XX != 0, 1
            // Message complete, response written into s5. Post code might not be set back to idle yet, but it will happen.
            return Ok(());
        }

        if s5 == 0 {
            // not yet booted or L2 init or old FW finished boot, or old FW processing message
            // or pcie_dma_{chip_to_host,host_to_chip}_transfer completed

            // Some of these also represent short-term busy, but it's safe to write SCRATCH[5].
            let pc_idle = pc == POST_CODE_INIT_DONE
                || (POST_CODE_ARC_MSG_HANDLE_DONE..=POST_CODE_ARC_TIME_LAST).contains(&pc);
            if pc_idle {
                return Ok(());
            } else {
                return Err(PlatformError::ArcNotReady(
                    crate::error::ArcReadyError::OldPostCode(pc),
                    BtWrapper::capture(),
                ))?;
            }
        }

        // We should never get here, every case should be handled above.
        Ok(())
    }

    pub fn spi_write(&self, addr: u32, value: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        let spi = super::spi::ActiveSpi::new(self, self.use_arc_for_spi)?;

        spi.write(self, addr, value)?;

        Ok(())
    }

    pub fn spi_read(&self, addr: u32, value: &mut [u8]) -> Result<(), Box<dyn std::error::Error>> {
        let spi = super::spi::ActiveSpi::new(self, self.use_arc_for_spi)?;

        spi.read(self, addr, value)?;

        Ok(())
    }
}

fn default_status() -> InitStatus {
    InitStatus {
        comms_status: super::CommsStatus::CanCommunicate,
        arc_status: ComponentStatusInfo {
            name: "ARC".to_string(),
            wait_status: Box::new([WaitStatus::Waiting(None)]),

            start_time: std::time::Instant::now(),
            timeout: std::time::Duration::from_secs(300),
        },
        dram_status: ComponentStatusInfo::init_waiting(
            "DRAM".to_string(),
            std::time::Duration::from_secs(300),
            4,
        ),
        eth_status: ComponentStatusInfo::init_waiting(
            "ETH".to_string(),
            std::time::Duration::from_secs(15 * 60),
            16,
        ),
        cpu_status: ComponentStatusInfo::not_present("CPU".to_string()),

        init_options: InitOptions { noc_safe: false },

        unknown_state: false,
    }
}

impl ChipImpl for Wormhole {
    fn update_init_state(
        &mut self,
        status: &mut InitStatus,
    ) -> Result<ChipInitResult, PlatformError> {
        if status.unknown_state {
            let init_options = std::mem::take(&mut status.init_options);
            *status = default_status();
            status.init_options = init_options;
        }

        let comms = &mut status.comms_status;

        {
            let status = &mut status.arc_status;
            for arc_status in status.wait_status.iter_mut() {
                match arc_status {
                    WaitStatus::Waiting(status_string) => {
                        match self.check_arc_msg_safe(5, 3) {
                            Ok(_) => *arc_status = WaitStatus::JustFinished,
                            Err(err) => {
                                match err {
                                    PlatformError::ArcNotReady(reason, _) => {
                                        // There are three possibilities when trying to get a response
                                        // here. 1. 0xffffffff in this case we want to assume this is some
                                        // sort of AxiError and abort the init. 2. An error we may
                                        // eventually recover from, i.e. arc booting... 3. we have hit an
                                        // error that won't resolve but isn't indicative of further
                                        // problems. For example watchdog triggered.
                                        match reason {
                                            // This is triggered when the s5 or pc registers readback
                                            // 0xffffffff. I am treating it like an AXI error and will
                                            // assume something has gone terribly wrong and abort.
                                            crate::error::ArcReadyError::NoAccess
                                            | crate::error::ArcReadyError::BootError
                                            | crate::error::ArcReadyError::WatchdogTriggered
                                            | crate::error::ArcReadyError::Asleep
                                            | crate::error::ArcReadyError::OldPostCode(_) => {
                                                *arc_status = WaitStatus::Error(super::init::status::ArcInitError::WaitingForInit(reason));
                                            }
                                            crate::error::ArcReadyError::BootIncomplete
                                            | crate::error::ArcReadyError::OutstandingPcieDMA
                                            | crate::error::ArcReadyError::MessageQueued(_)
                                            | crate::error::ArcReadyError::HandlingMessage(_) => {
                                                *status_string = Some(reason.to_string());
                                                if status.start_time.elapsed() > status.timeout {
                                                    *arc_status = WaitStatus::Error(super::init::status::ArcInitError::WaitingForInit(reason));
                                                }
                                            }
                                        }
                                    }

                                    PlatformError::UnsupportedFwVersion { version, required } => {
                                        *arc_status = WaitStatus::Error(
                                            super::init::status::ArcInitError::FwVersionTooOld {
                                                version,
                                                required,
                                            },
                                        );
                                    }

                                    // The fact that this is here means that our result is too generic, for now we just ignore it.
                                    PlatformError::ArcMsgError(error) => {
                                        return Ok(ChipInitResult::ErrorContinue(
                                            error.to_string(),
                                            backtrace::Backtrace::capture(),
                                        ));
                                    }

                                    PlatformError::MessageError(error) => {
                                        return Ok(ChipInitResult::ErrorContinue(
                                            error.to_string(),
                                            backtrace::Backtrace::capture(),
                                        ));
                                    }

                                    // This is fine to hit at this stage (though it should have been already verified to not be the case).
                                    // For now we just ignore it and hope that it will be resolved by the time the timeout expires...
                                    PlatformError::EthernetTrainingNotComplete(_) => {
                                        if let WaitStatus::Waiting(status_string) = arc_status {
                                            if status.start_time.elapsed() > status.timeout {
                                                *arc_status = WaitStatus::Timeout(status.timeout);
                                            } else {
                                                *status_string = Some("Waiting on arc/ethernet; this is unexpected but we'll assume that things will clear up if we wait.".to_string());
                                            }
                                        }
                                    }

                                    // This is an "expected error" but we probably can't recover from it, so we should abort the init.
                                    PlatformError::AxiError(error) => {
                                        *comms = CommsStatus::CommunicationError(error.to_string());
                                        return Ok(ChipInitResult::ErrorAbort(
                                            format!("ARC AXI error: {error}"),
                                            backtrace::Backtrace::capture(),
                                        ));
                                    }

                                    // We don't expect to hit these cases so if we do, we should assume that something went terribly
                                    // wrong and abort the init.
                                    PlatformError::WrongChipArch {
                                        actual,
                                        expected,
                                        backtrace,
                                    } => {
                                        return Ok(ChipInitResult::ErrorAbort(
                                            format!(
                                                "expected chip: {expected}, actual detected chip: {actual}"
                                            ),
                                            backtrace.0,
                                        ))
                                    }

                                    PlatformError::WrongChipArchs {
                                        actual,
                                        expected,
                                        backtrace,
                                    } => {
                                        let expected_chips = expected
                                            .iter()
                                            .map(|arch| arch.to_string())
                                            .collect::<Vec<_>>()
                                            .join(", ");
                                        return Ok(ChipInitResult::ErrorAbort(
                                            format!(
                                                "expected chip: {expected_chips}, actual detected chips: {actual}"
                                            ),
                                            backtrace.0,
                                        ));
                                    }

                                    PlatformError::Generic(error, backtrace) => {
                                        return Ok(ChipInitResult::ErrorAbort(error, backtrace.0));
                                    }

                                    PlatformError::GenericError(error, backtrace) => {
                                        let err_msg = error.to_string();
                                        return Ok(ChipInitResult::ErrorAbort(
                                            err_msg,
                                            backtrace.0,
                                        ));
                                    }
                                }
                            }
                        }
                    }
                    WaitStatus::JustFinished => {
                        *arc_status = WaitStatus::Done;
                    }
                    _ => {}
                }
            }
        }

        // If ARC has not finished initialization then we shouldn't init eth or dram.
        if !status.arc_status.is_waiting() {
            // If something went wrong with ARC then we probably don't have DRAM
            if !status.arc_status.has_error() {
                let arc_status = &mut status.arc_status;
                let status = &mut status.dram_status;

                // We don't need to get the telemetry if we aren't waiting to see if dram has
                // trained...
                if status.is_waiting() {
                    // Hitting an error here implies that the something changed and we no longer have arc
                    // access. We will abort, but allow other chips to continue or not using the typical
                    // switch block.
                    let telem = match self.get_telemetry() {
                        Ok(telem) => Some(telem),
                        Err(err) => match err {
                            // Something did go wrong here, but we'll assume that this is a result of
                            // some temporary issue
                            PlatformError::ArcNotReady(_, _)
                            // Not really expected but ethernet training may not yet be complete so
                            // we'll just ignore it for now.
                            | PlatformError::EthernetTrainingNotComplete(_) => {
                                for dram_status in status.wait_status.iter_mut() {
                                    if let WaitStatus::Waiting(status_string) = dram_status {
                                        if status.start_time.elapsed() > status.timeout {
                                            *dram_status = WaitStatus::Timeout(status.timeout);
                                        } else {
                                            *status_string = Some("Waiting on arc/ethernet; this is unexpected but we'll assume that things will clear up if we wait.".to_string());
                                        }
                                    }
                                }

                                None
                            }


                            PlatformError::UnsupportedFwVersion { version, required } => {
                                if let Some(status) = arc_status.wait_status.get_mut(0) {
                                    *status = WaitStatus::Error(crate::chip::init::status::ArcInitError::FwVersionTooOld {
                                        version,
                                        required,
                                    });
                                }
                                None
                            }

                            // Arc should be ready here...
                            // This means that ARC hung, we should stop initializing this chip in case
                            // we hit something like a noc hang.
                            PlatformError::ArcMsgError(error) => {
                                return Ok(ChipInitResult::ErrorContinue(format!("Telemetry ARC message error: {error}; we expected to have communication, but lost it."), backtrace::Backtrace::capture()));
                            }

                            PlatformError::MessageError(error) => {
                                return Ok(ChipInitResult::ErrorContinue(format!("Telemetry ARC message error: {error:?}; we expected to have communication, but lost it."), backtrace::Backtrace::capture()));
                            }

                            // This is an "expected error" but we probably can't recover from it, so we should abort the init.
                            PlatformError::AxiError(error) => return Ok(ChipInitResult::ErrorAbort(error.to_string(), backtrace::Backtrace::capture())),

                            // We don't expect to hit these cases so if we do, we should assume that something went terribly
                            // wrong and abort the init.
                            PlatformError::WrongChipArch {actual, expected, backtrace} => {
                                return Ok(ChipInitResult::ErrorAbort(format!("expected chip: {expected}, actual detected chip: {actual}"), backtrace.0))
                            }

                            PlatformError::WrongChipArchs {actual, expected, backtrace} => {
                                let expected_chips = expected.iter().map(|arch| arch.to_string()).collect::<Vec<_>>().join(", ");
                                return Ok(ChipInitResult::ErrorAbort(format!("expected chip: {expected_chips}, actual detected chips: {actual}"), backtrace.0));
                            }

                            PlatformError::Generic(error, backtrace) => {
                                return Ok(ChipInitResult::ErrorAbort(error, backtrace.0));
                            }

                            | PlatformError::GenericError(error, backtrace) => {
                                let err_msg = error.to_string();
                                return Ok(ChipInitResult::ErrorAbort(err_msg, backtrace.0));
                            }
                        },
                    };

                    if let Some(telem) = telem {
                        let dram_status = telem.ddr_status;

                        let mut channels = [None; 6];
                        for (i, channel) in channels.iter_mut().enumerate() {
                            let status = (dram_status >> (i * 4)) & 0xF;
                            let status = status as u8;

                            *channel =
                                super::init::status::DramChannelStatus::try_from(status).ok();
                        }

                        for (dram_status, channel_status) in
                            status.wait_status.iter_mut().zip(channels)
                        {
                            match dram_status {
                                WaitStatus::Waiting(status_string) => {
                                    if let Some(
                                        super::init::status::DramChannelStatus::TrainingPass,
                                    ) = channel_status
                                    {
                                        *dram_status = WaitStatus::Done;
                                    } else {
                                        *status_string = Some(
                                            channel_status
                                                .map(|v| v.to_string())
                                                .unwrap_or("Unknown".to_string()),
                                        );
                                        if status.start_time.elapsed() > status.timeout {
                                            if let Some(status) = channel_status {
                                                *dram_status = WaitStatus::Error(
                                                    super::init::status::DramInitError::NotTrained(
                                                        status,
                                                    ),
                                                );
                                            } else {
                                                *dram_status = WaitStatus::Timeout(status.timeout);
                                            }
                                        }
                                    }
                                }
                                WaitStatus::JustFinished => {
                                    *dram_status = WaitStatus::Done;
                                }
                                _ => {}
                            }
                        }
                    }
                }
            } else {
                for dram_status in status.dram_status.wait_status.iter_mut() {
                    *dram_status = WaitStatus::NoCheck;
                }
            }
        } else {
            for dram_status in status.dram_status.wait_status.iter_mut() {
                if let WaitStatus::Waiting(status_string) = dram_status {
                    *status_string = Some("Waiting for ARC".to_string());
                }
            }
        }

        // If ARC has not finished initialization then we shouldn't init eth.
        if !status.arc_status.is_waiting() {
            // We need arc to be alive so that we can check which cores are enabled
            if !status.arc_status.has_error() {
                // Only do eth training if board type is not UBB
                // By this point arc should be alive so we can safely access telem
                let telem = self.get_telemetry()?;
                let board_type: u64 =
                    telem.board_id_low as u64 | ((telem.board_id_high as u64) << 32);
                let board_upi: u64 = (board_type >> 36) & 0xFFFFF;
                const WH_6U_GLX_UPI: u64 = 0x35;

                if board_upi != WH_6U_GLX_UPI {
                    // Only try to initialize the ethernet if we are not in noc_safe mode.
                    if !status.init_options.noc_safe {
                        let status = &mut status.eth_status;

                        // We don't need to get the eth training status if we aren't waiting to see if dram has
                        // trained...
                        if status.is_waiting() {
                            let eth_training_status = match self.check_ethernet_training_complete() {
                                Ok(eth_status) => eth_status,
                                Err(err) => match err {
                                    // ARC should be initialized at this point, hitting an error here means
                                    // that we can no longer progress in the init.
                                    PlatformError::ArcMsgError(error) => {
                                        return Ok(ChipInitResult::ErrorContinue(
                                            error.to_string(),
                                            backtrace::Backtrace::capture(),
                                        ));
                                    }

                                    PlatformError::MessageError(error) => {
                                        return Ok(ChipInitResult::ErrorContinue(
                                            error.to_string(),
                                            backtrace::Backtrace::capture(),
                                        ));
                                    }

                                    PlatformError::ArcNotReady(error, backtrace) => {
                                        return Ok(ChipInitResult::ErrorContinue(
                                            error.to_string(),
                                            backtrace.0,
                                        ));
                                    }

                                    // We are checking for ethernet training to complete... if we hit this than
                                    // something has gone terribly wrong
                                    PlatformError::EthernetTrainingNotComplete(eth_cores) => {
                                        let false_count = eth_cores.iter().filter(|&&x| !x).count();
                                        return Ok(ChipInitResult::ErrorContinue(
                                            format!(
                                                "Ethernet training not complete on [{false_count}/16] ports"
                                            ),
                                            backtrace::Backtrace::capture(),
                                        ));
                                    }

                                    // This is an "expected error" but we probably can't recover from it, so we should abort the init.
                                    PlatformError::AxiError(error) => {
                                        return Ok(ChipInitResult::ErrorAbort(
                                            error.to_string(),
                                            backtrace::Backtrace::capture(),
                                        ));
                                    }

                                    // We don't expect to hit these cases so if we do, we should assume that something went terribly
                                    // wrong and abort the init.
                                    PlatformError::UnsupportedFwVersion { version, required } => {
                                        return Ok(ChipInitResult::ErrorAbort(format!("Required Ethernet Firmware Version: {required}, current version: {version:?}"), backtrace::Backtrace::capture()));
                                    }
                                    PlatformError::WrongChipArch {
                                        actual,
                                        expected,
                                        backtrace,
                                    } => {
                                        return Ok(ChipInitResult::ErrorAbort(
                                            format!(
                                            "expected chip: {expected}, actual detected chip: {actual}"
                                        ),
                                            backtrace.0,
                                        ))
                                    }

                                    PlatformError::WrongChipArchs {
                                        actual,
                                        expected,
                                        backtrace,
                                    } => {
                                        let expected_chips = expected
                                            .iter()
                                            .map(|arch| arch.to_string())
                                            .collect::<Vec<_>>()
                                            .join(", ");
                                        return Ok(ChipInitResult::ErrorAbort(
                                            format!(
                                                "expected chip: {expected_chips}, actual detected chips: {actual}"
                                            ),
                                            backtrace.0,
                                        ));
                                    }

                                    PlatformError::Generic(error, backtrace) => {
                                        return Ok(ChipInitResult::ErrorAbort(error, backtrace.0));
                                    }

                                    PlatformError::GenericError(error, backtrace) => {
                                        let err_msg = error.to_string();
                                        return Ok(ChipInitResult::ErrorAbort(err_msg, backtrace.0));
                                    }
                                },
                            };
                            for (eth_status, training_complete) in
                                status.wait_status.iter_mut().zip(eth_training_status)
                            {
                                match eth_status {
                                    WaitStatus::Waiting(status_string) => {
                                        if training_complete {
                                            if let Err(_err) = self.check_ethernet_fw_version() {
                                                *eth_status = WaitStatus::NotInitialized(
                                                    EthernetPartialInitError::FwOverwritten,
                                                );
                                            } else {
                                                *eth_status = WaitStatus::JustFinished;
                                            }
                                        } else if status.start_time.elapsed() > status.timeout {
                                            *eth_status = WaitStatus::Timeout(status.timeout);
                                        } else {
                                            *status_string = Some(format!(
                                                "{}: Waiting for initial training to complete",
                                                self.get_local_chip_coord()?
                                            ));
                                        }
                                    }
                                    WaitStatus::JustFinished => {
                                        *eth_status = WaitStatus::Done;
                                    }
                                    _ => {}
                                }
                            }
                        }
                    } else {
                        let status = &mut status.eth_status;
                        for eth_status in status.wait_status.iter_mut() {
                            *eth_status = WaitStatus::Done;
                        }
                    }
                } else {
                    // If WH UBB - skip ethernet training check
                    let status = &mut status.eth_status;
                    for eth_status in status.wait_status.iter_mut() {
                        *eth_status = WaitStatus::Done;
                    }
                }
            } else {
                let status = &mut status.eth_status;
                for eth_status in status.wait_status.iter_mut() {
                    *eth_status = WaitStatus::NoCheck;
                }
            }
        } else {
            for eth_status in status.eth_status.wait_status.iter_mut() {
                if let WaitStatus::Waiting(status_string) = eth_status {
                    *status_string = Some("Waiting for ARC".to_string());
                }
            }
        }

        {
            // This is not present in wormhole.
            let _status = &mut status.cpu_status;
        }

        Ok(ChipInitResult::NoError)
    }

    fn get_arch(&self) -> luwen_def::Arch {
        luwen_def::Arch::Wormhole
    }

    fn arc_msg(&self, msg: ArcMsgOptions) -> Result<ArcMsgOk, PlatformError> {
        let (msg_reg, return_reg) = if msg.use_second_mailbox {
            (2, 4)
        } else {
            (5, 3)
        };

        self.check_arc_msg_safe(msg_reg, return_reg)?;

        crate::arc_msg::arc_msg(
            self,
            &msg.msg,
            msg.wait_for_done,
            msg.timeout,
            msg_reg,
            return_reg,
            msg.addrs.as_ref().unwrap_or(&self.arc_addrs),
        )
    }

    fn get_neighbouring_chips(&self) -> Result<Vec<NeighbouringChip>, crate::error::PlatformError> {
        const ETH_UNKNOWN: u32 = 0;
        const ETH_UNCONNECTED: u32 = 1;
        const ETH_NO_ROUTING: u32 = 2;

        const SHELF_OFFSET: u64 = 9;
        const RACK_OFFSET: u64 = 10;

        let mut output = Vec::with_capacity(self.eth_locations.len());

        for (
            eth_id,
            EthCore {
                x: eth_x, y: eth_y, ..
            },
        ) in self.eth_locations.iter().copied().enumerate()
        {
            let port_status = self.arc_if.noc_read32(
                &self.chip_if,
                0,
                eth_x,
                eth_y,
                self.eth_addrs.eth_conn_info + (eth_id as u64 * 4),
            )?;

            if port_status == ETH_UNCONNECTED || port_status == ETH_UNKNOWN {
                continue;
            }

            // HACK(drosen): It's not currently possible to route galaxy->nb...
            // This is a limitation of the current ethernet firmware routing scheme,
            // but fixing it would require a large-ish firmware update and lots of testing so
            // for now we are just ignoring those routes.

            // Get the neighbour's board type
            let next_board_type = self.noc_read32(
                0,
                eth_x,
                eth_y,
                0x1ec0 + (self.eth_addrs.erisc_remote_board_type_offset * 4),
            )?;

            // Get the our board type
            let our_board_type = self.noc_read32(
                0,
                eth_x,
                eth_y,
                0x1ec0 + (self.eth_addrs.erisc_local_board_type_offset * 4),
            )?;

            // Check if it's possible to have routing disabled
            let erisc_routing_disabled =
                self.noc_read32(0, eth_x, eth_y, self.eth_addrs.boot_params + (19 * 4))? == 1;

            // The board type value will be 0 if galaxy and non-zero if nb
            // It's currently not possible to go from GALAXY->NB
            let routing_disabled = (our_board_type == 0 && next_board_type != 0)
                || (erisc_routing_disabled && port_status == ETH_NO_ROUTING);

            // Decode the remote eth_addr for our erisc core
            // This can be used to build a map of the full mesh
            let remote_id = self.noc_read32(
                0,
                eth_x,
                eth_y,
                self.eth_addrs.node_info + (4 * RACK_OFFSET),
            )?;
            let remote_rack_x = remote_id & 0xFF;
            let remote_rack_y = (remote_id >> 8) & 0xFF;

            let remote_id = self.noc_read32(
                0,
                eth_x,
                eth_y,
                self.eth_addrs.node_info + (4 * SHELF_OFFSET),
            )?;
            let remote_shelf_x = (remote_id >> 16) & 0x3F;
            let remote_shelf_y = (remote_id >> 22) & 0x3F;

            let remote_noc_x = (remote_id >> 4) & 0x3F;
            let remote_noc_y = (remote_id >> 10) & 0x3F;

            output.push(NeighbouringChip {
                routing_enabled: !routing_disabled,
                local_noc_addr: (eth_x, eth_y),
                remote_noc_addr: (remote_noc_x as u8, remote_noc_y as u8),
                eth_addr: EthAddr {
                    shelf_x: remote_shelf_x as u8,
                    shelf_y: remote_shelf_y as u8,
                    rack_x: remote_rack_x as u8,
                    rack_y: remote_rack_y as u8,
                },
            });
        }

        Ok(output)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn get_telemetry(&self) -> Result<super::Telemetry, PlatformError> {
        // Try to initialise (and cache) the new BH-style telemetry.
        let new_telem = self
            .new_telemetry
            .get_or_try_init(|| self.try_init_new_telemetry())?;

        if let Some(new_telem) = new_telem {
            self.get_telemetry_new(new_telem)
        } else {
            self.get_telemetry_legacy()
        }
    }

    fn get_device_info(&self) -> Result<Option<crate::DeviceInfo>, PlatformError> {
        if self.is_remote {
            Ok(None)
        } else {
            Ok(self.chip_if.get_device_info()?)
        }
    }
}
