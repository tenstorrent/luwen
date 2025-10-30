// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use luwen_api::{
    chip::{ArcMsgOptions, Chip, HlComms, HlCommsInterface},
    CallbackStorage, ChipImpl, TypedArcMsg,
};
use luwen_pci::{
    comms_callback, error::LuwenError, DmaConfig, ExtendedPciDevice, ExtendedPciDeviceWrapper,
    PciDevice, Tlb,
};

pub fn main() -> Result<(), LuwenError> {
    let mut chips = Vec::new();

    let device_ids = PciDevice::scan();
    for device_id in device_ids {
        let ud = ExtendedPciDevice::open(device_id)?;
        let arch = ud.borrow().device.arch;

        let chip = Chip::open(arch, CallbackStorage::new(comms_callback, ud))?;

        if let Some(wh) = chip.as_wh() {
            let hi = wh
                .chip_if
                .as_any()
                .downcast_ref::<CallbackStorage<ExtendedPciDeviceWrapper>>()
                .unwrap();

            let turbo_data = {
                let pci_interface: &mut ExtendedPciDevice = &mut hi.user_data.borrow_mut();
                let mut buffer = pci_interface.device.allocate_dma_buffer(0x1000)?;

                let dma_request = wh.axi_translate("ARC_CSM.ARC_PCIE_DMA_REQUEST")?;
                let arc_misc_cntl = wh.axi_translate("ARC_RESET.ARC_MISC_CNTL")?;

                pci_interface.device.dma_config = Some(DmaConfig {
                    csm_pcie_ctrl_dma_request_offset: dma_request.addr as u32,
                    arc_misc_cntl_addr: arc_misc_cntl.addr as u32,
                    dma_host_phys_addr_high: 0,
                    support_64_bit_dma: false,
                    use_msi_for_dma: false,
                    read_threshold: 0,
                    write_threshold: 0,
                });

                let (offset, _size) = pci_interface.device.setup_tlb(
                    &luwen_kmd::PossibleTlbAllocation::Hardcoded(168),
                    Tlb {
                        local_offset: 0x0,
                        x_end: 1,
                        y_end: 1,
                        ..Default::default()
                    },
                )?;

                let mut index = 0;
                buffer.buffer.fill_with(|| {
                    index += 1;
                    index as u8
                });

                pci_interface.device.pcie_dma_transfer_turbo(
                    offset as u32,
                    buffer.physical_address,
                    0x1000,
                    true,
                )?;

                buffer.buffer.fill(0);

                pci_interface.device.pcie_dma_transfer_turbo(
                    offset as u32,
                    buffer.physical_address,
                    0x1000,
                    false,
                )?;

                buffer.buffer.iter().copied().collect::<Vec<_>>()
            };

            let mut data = [0; 0x1000];
            wh.noc_read(0, 1, 1, 0x0, &mut data).unwrap();

            for (i, d) in data.iter().enumerate() {
                if *d != (i + 1) as u8 {
                    panic!("Mismatch at index {i}");
                }
            }

            for (i, d) in turbo_data.iter().enumerate() {
                if *d != (i + 1) as u8 {
                    panic!("Mismatch at index {i}");
                }
            }
        }
    }

    let device_ids = PciDevice::scan();
    for device_id in device_ids {
        println!("Running on device {device_id}");
        let ud = ExtendedPciDevice::open(device_id)?;
        let arch = ud.borrow().device.arch;

        let chip = Chip::open(arch, CallbackStorage::new(comms_callback, ud))?;

        chip.arc_msg(ArcMsgOptions {
            msg: TypedArcMsg::Test { arg: 101 }.into(),
            ..Default::default()
        })?;

        if let Some(wh) = chip.as_wh() {
            let remote_wh = wh.open_remote((1, 0)).unwrap();

            remote_wh.arc_msg(ArcMsgOptions {
                msg: TypedArcMsg::Test { arg: 101 }.into(),
                ..Default::default()
            })?;
        }

        chips.push(chip);
    }

    let all_chips = luwen_api::detect_chips_silent(chips, Default::default())?;
    for (chip_id, chip) in all_chips.into_iter().enumerate() {
        println!("Running on device {chip_id}");
        chip.arc_msg(ArcMsgOptions {
            msg: TypedArcMsg::Test { arg: 101 }.into(),
            ..Default::default()
        })?;
    }

    Ok(())
}
