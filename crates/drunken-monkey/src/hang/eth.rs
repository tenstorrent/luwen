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
    let eth_locations = [
        (9, 0),
        (1, 0),
        (8, 0),
        (2, 0),
        (7, 0),
        (3, 0),
        (6, 0),
        (4, 0),
        (9, 6),
        (1, 6),
        (8, 6),
        (2, 6),
        (7, 6),
        (3, 6),
        (6, 6),
        (4, 6),
    ];

    let (noc_x, noc_y) = eth_locations[core as usize];

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
