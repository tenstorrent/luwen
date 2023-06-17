use ttchip::{ArcMsg, Wormhole, DmaConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut chip = Wormhole::create(1)?;

    chip.arc_msg(
        &mut ArcMsg::TEST { arg: 0x1000 },
        true,
        std::time::Duration::from_secs(1),
        false,
    )?;
    // let result = chip.ARC.read32("ARC_RESET.SCRATCH[5]");
    let result = chip.chip.transport.read32(0x1FF30000 + 0x0060 + 4 * 3)?;

    assert_eq!(result, 0x1001);

    chip.chip.transport.write_block(0x1FF30000 + 0x0060 + 4 * 3, &[0x00, 0x01, 0x02, 0x03])?;
    let mut output = [0; 4];
    chip.chip.transport.read_block(0x1FF30000 + 0x0060 + 4 * 3, &mut output)?;

    println!("{:?}", output);

    chip.chip.transport.dma_config = Some(DmaConfig {
        csm_pcie_ctrl_dma_request_offset: 535790792,
        arc_misc_cntl_addr: 536019200,
        dma_host_phys_addr_high: 0,
        support_64_bit_dma: false,
        use_msi_for_dma: false,
        read_theshold: 1,
        write_theshold: 1,
    });

    chip.chip.transport.write_block(0x1FF30000 + 0x0060 + 4 * 3, &[0x11, 0x11, 0x12, 0x13])?;
    let mut output = [0; 32];
    chip.chip.transport.read_block(0x1FF30000 + 0x0060 + 4 * 3, &mut output)?;

    println!("{:x?}", output);

    Ok(())
}
