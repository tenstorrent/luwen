use ttchip::{ArcMsg, DmaConfig, Wormhole};

pub fn scan() -> Result<(), Box<dyn std::error::Error>> {
    for chip in ttchip::scan()? {
        match chip {
            ttchip::AllChips::Wormhole(mut wh) => {
                println!(
                    "id {} {:X} is WH",
                    wh.chip.transport.id, wh.chip.transport.physical.device_id
                );

                wh.noc(false).write32(0x1, 0x1, 0x0, 0x401e)?;

                let mut remote = wh.remote((1, 0))?;

                remote.write32(1, 1, false, 0x0, 0xfaca)?;
                let value = remote.read32(1, 1, false, 0x0)?;
                assert_eq!(value, 0xfaca);

                assert_eq!(wh.noc(false).read32(1, 1, 0x0)?, 0x401e);
            }
            ttchip::AllChips::Grayskull(mut gs) => {
                println!(
                    "id {} {:X} is WH",
                    gs.chip.transport.id, gs.chip.transport.physical.device_id
                );

                gs.chip.arc_msg(&mut ArcMsg::TEST { arg: 0x1000 }, true, std::time::Duration::from_secs(1), false)?;
                let result: u32 = gs.chip.axi().read("ARC_RESET.SCRATCH[3]")?;
                assert_eq!(result, 0x1001);
            }
        }
    }

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut chip = Wormhole::create(1)?;

    chip.arc_msg(
        &mut ArcMsg::TEST { arg: 0x1000 },
        true,
        std::time::Duration::from_secs(1),
        false,
    )?;
    // let result = chip.chip.transport.read32(0x1FF30000 + 0x0060 + 4 * 3)?;
    let result: u32 = chip.chip.axi().read("ARC_RESET.SCRATCH[3]")?;
    assert_eq!(result, 0x1001);

    chip.chip
        .transport
        .write_block(0x1FF30000 + 0x0060 + 4 * 3, &[0x00, 0x01, 0x02, 0x03])?;
    let mut output = [0; 4];
    chip.chip
        .transport
        .read_block(0x1FF30000 + 0x0060 + 4 * 3, &mut output)?;
    assert_eq!(output, [0x00, 0x01, 0x02, 0x03]);

    chip.chip.transport.dma_config = Some(DmaConfig {
        csm_pcie_ctrl_dma_request_offset: 535790792,
        arc_misc_cntl_addr: 536019200,
        dma_host_phys_addr_high: 0,
        support_64_bit_dma: false,
        use_msi_for_dma: false,
        read_theshold: 1,
        write_theshold: 1,
    });

    chip.chip
        .transport
        .write_block(0x1FF30000 + 0x0060 + 4 * 3, &[0x11, 0x11, 0x12, 0x13])?;
    let mut output = [0; 32];
    chip.chip
        .transport
        .read_block(0x1FF30000 + 0x0060 + 4 * 3, &mut output)?;
    assert_eq!(
        output,
        [
            0x11, 0x11, 0x12, 0x13, 0, 0, 0x11, 0x11, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x8a, 0,
            0, 0, 0x12, 1, 0, 0, 0x20, 1, 0, 0
        ]
    );

    chip.noc(false).write32(1, 1, 0x0, 0xfaca)?;
    let value = chip.noc(false).read32(1, 1, 0x0)?;
    assert_eq!(value, 0xfaca);

    chip.noc(false)
        .block_write(1, 1, 0x100, &[0x00, 0x01, 0x02, 0x03])?;
    let mut output = [0; 4];
    chip.noc(false).block_read(1, 1, 0x100, &mut output)?;
    assert_eq!(output, [0x00, 0x01, 0x02, 0x03]);

    scan()?;

    Ok(())
}
