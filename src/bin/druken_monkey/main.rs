use luwen_if::{
    chip::{eth_addr, Chip, HlComms, HlCommsInterface},
    ArcState, ChipImpl,
};
use luwen_ref::detect_initialized_chips;

pub enum ArcHangMethod {
    OverwriteFwCode,
    A5,
    CoreHault,
}

fn hang_arc(method: ArcHangMethod, chip: Chip) -> Result<(), Box<dyn std::error::Error>> {
    match method {
        ArcHangMethod::OverwriteFwCode => {
            unimplemented!("Haven't implemented fw overrwrite");
        }
        ArcHangMethod::A5 => {
            chip.arc_msg(luwen_if::chip::ArcMsgOptions {
                msg: luwen_if::ArcMsg::SetArcState {
                    state: ArcState::A5,
                },
                ..Default::default()
            })?;
        }
        ArcHangMethod::CoreHault => {
            // Need to go into arc a3 before haulting the core, otherwise we can interrupt
            // communication with the voltage regulator.
            chip.arc_msg(luwen_if::chip::ArcMsgOptions {
                msg: luwen_if::ArcMsg::SetArcState {
                    state: ArcState::A3,
                },
                ..Default::default()
            })?;

            let rmw = chip.axi_sread32("ARC_RESET.ARC_MISC_CNTL")?;
            // the core hault bits are in 7:4 we only care about core 0 here (aka bit 4)
            chip.axi_swrite32("ARC_RESET.ARC_MISC_CNTL", rmw | (1 << 4))?;
        }
    }

    Ok(())
}

pub enum NocHangMethod {
    AccessCgRow,
    AccessNonExistantEndpoint,
}

fn hang_noc(method: NocHangMethod, chip: Chip) -> Result<(), Box<dyn std::error::Error>> {
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

pub enum EthHangMethod {
    OverwriteFwVersion,
    OverwriteEthFw,
}

fn hang_eth(
    method: EthHangMethod,
    core: u32,
    chip: Chip,
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

fn main() {
    let args = std::env::args().collect::<Vec<_>>();

    let command = args[1].clone();
    let option = args.get(2);

    let mut chips = detect_initialized_chips().unwrap();

    match command.as_str() {
        "arc" => {
            let option = option.map(|v| v.as_str()).unwrap_or("overwrite");
            let method = match option {
                "overwrite" => ArcHangMethod::OverwriteFwCode,
                "a5" => ArcHangMethod::A5,
                "hault" => ArcHangMethod::CoreHault,
                other => {
                    unimplemented!("Have not yet implemented support for arc hang method {other}")
                }
            };
            hang_arc(method, chips.pop().unwrap()).unwrap()
        }
        "noc" => {
            let option = option.map(|v| v.as_str()).unwrap_or("cg");
            let method = match option {
                "cg" => NocHangMethod::AccessCgRow,
                "oob" => NocHangMethod::AccessNonExistantEndpoint,
                other => {
                    unimplemented!("Have not implemented support for noc hang method {other}")
                }
            };
            hang_noc(method, chips.pop().unwrap()).unwrap();
        }
        "eth" => {
            let option = option.map(|v| v.as_str()).unwrap_or("ver");
            let method = match option {
                "ver" => EthHangMethod::OverwriteFwVersion,
                "fw" => EthHangMethod::OverwriteEthFw,
                other => {
                    unimplemented!("Have not yet implementd support for eth hang method {other}");
                }
            };
            let core = args.get(3).map(|v| v.parse()).unwrap_or(Ok(0)).unwrap();
            hang_eth(method, core, chips.pop().unwrap()).unwrap();
        }
        other => unimplemented!("Have not yet implemented support for command {other}"),
    }
}
