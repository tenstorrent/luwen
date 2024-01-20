use clap::ValueEnum;
use luwen_if::chip::{HlComms, Wormhole};

#[derive(Debug, Clone, ValueEnum)]
pub enum EthHangMethod {
    OverwriteFwVersion,
    OverwriteEthFw,
}

pub fn hang_eth(
    method: EthHangMethod,
    core: u32,
    chip: &Wormhole,
) -> Result<(), Box<dyn std::error::Error>> {
    let (noc_x, noc_y) = (
        chip.eth_locations[core as usize].x,
        chip.eth_locations[core as usize].y,
    );

    match method {
        EthHangMethod::OverwriteFwVersion => {
            chip.noc_write32(0, noc_x, noc_y, 0x210, 0xdeadbeef)?;
        }
        EthHangMethod::OverwriteEthFw => {
            let data = [0u8; 1000];
            chip.noc_write(0, noc_y, noc_x, 0x0, &data)?;
        }
    }

    Ok(())
}
