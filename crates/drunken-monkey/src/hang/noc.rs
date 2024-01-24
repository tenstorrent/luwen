use clap::ValueEnum;
use luwen_if::chip::{Chip, HlComms};

#[derive(Debug, Clone, ValueEnum)]
pub enum NocHangMethod {
    AccessCgRow,
    AccessNonExistantEndpoint,
}

pub fn hang_noc(method: NocHangMethod, chip: Chip) -> Result<(), Box<dyn std::error::Error>> {
    match method {
        NocHangMethod::AccessCgRow => {
            let noc_x = 18;
            let noc_y = 18;
            let cfg_addr = 0xffb30100;

            // Enable clock gating on physical row 12
            let rmw = chip.noc_read32(1, noc_x, noc_y, cfg_addr)?;
            chip.noc_write32(1, noc_x, noc_y, cfg_addr, rmw | (1 << 12))?;

            for _ in 0..100 {
                chip.noc_read32(1, noc_x, noc_y, 0x100)?;
            }
        }
        NocHangMethod::AccessNonExistantEndpoint => {
            // We have some capacity to deal with outstanding transactions
            // but 100 accesses is enough to overcome that capability
            for _ in 0..100 {
                // The total grid size is 10x12 with a virtual noc grid starting at coord [26, 26]
                // We are there trying to access a register that doesn't exist
                chip.noc_read32(0, 1, 13, 0x0)?;
            }
        }
    }

    Ok(())
}
