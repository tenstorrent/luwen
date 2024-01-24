use drunken_monkey::hang;

mod clap;

fn main() {
    let args = <clap::CliOptions as ::clap::Parser>::parse();

    let chips = luwen_ref::detect_chips().unwrap();

    match args.ty {
        clap::HangType::Arc { method } => {
            for chip in chips {
                hang::arc::hang_arc(method.clone(), chip).unwrap();
            }
        }
        clap::HangType::Noc { method } => {
            for chip in chips {
                hang::noc::hang_noc(method.clone(), chip).unwrap();
            }
        }
        clap::HangType::Eth { method, eth_id } => {
            for chip in chips {
                if let Some(wh) = chip.as_wh() {
                    hang::eth::hang_eth(method.clone(), eth_id, wh).unwrap();
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use drunken_monkey::hang::{
        arc::{hang_arc, ArcHangMethod},
        eth::{hang_eth, EthHangMethod},
        noc::{hang_noc, NocHangMethod},
    };

    fn arc_hang(method: ArcHangMethod) {
        let chips = luwen_ref::detect_chips().unwrap();

        let mut expected_chip_count = 0;
        for chip in chips {
            if let Some(wh) = chip.as_wh() {
                if !wh.is_remote {
                    expected_chip_count += 1;
                }
            } else {
                expected_chip_count += 1;
            }
            hang_arc(method.clone(), chip).unwrap();
        }

        let chips = luwen_ref::detect_chips_fallible().unwrap();

        assert_eq!(
            chips.len(),
            expected_chip_count,
            "{:?}",
            chips.iter().map(|v| v.status()).collect::<Vec<_>>()
        );

        for chip in chips {
            if let Some(chip) = chip.status() {
                if !chip.arc_status.has_error() {
                    panic!("ARC should be hung but wasn't: {chip:?}");
                }
            } else {
                panic!("Found chip that still initialized after arc was hung");
            }
        }
    }

    #[test]
    #[ignore = "requires reset dongle"]
    fn a5_arc_hang() {
        arc_hang(ArcHangMethod::A5);
    }

    #[test]
    #[ignore = "requires reset dongle"]
    fn core_hault_arc_hang() {
        arc_hang(ArcHangMethod::CoreHault);
    }

    #[test]
    #[ignore = "requires reset dongle"]
    fn fw_overwrite_arc_hang() {
        arc_hang(ArcHangMethod::OverwriteFwCode);
    }

    fn noc_hang(method: NocHangMethod) {
        let chips = luwen_ref::detect_chips().unwrap();

        let mut expected_chip_count = 0;
        for chip in chips {
            if let Some(wh) = chip.as_wh() {
                if !wh.is_remote {
                    expected_chip_count += 1;
                }
            } else {
                expected_chip_count += 1;
            }
            hang_noc(method.clone(), chip).unwrap();
        }

        let chips = luwen_ref::detect_chips_fallible().unwrap();

        assert_eq!(
            chips.len(),
            expected_chip_count,
            "{:?}",
            chips.iter().map(|v| v.status()).collect::<Vec<_>>()
        );

        for chip in chips {
            if let Some(chip) = chip.status() {
                if !chip.arc_status.has_error() {
                    panic!("ARC should be hung but wasn't: {chip:?}");
                }
            } else {
                panic!("Found chip that still initialized after arc was hung");
            }
        }
    }

    #[test]
    #[ignore = "requires reset dongle"]
    fn cg_noc_hang() {
        noc_hang(NocHangMethod::AccessCgRow);
    }

    #[test]
    #[ignore = "requires reset dongle"]
    fn invalid_address_noc_hang() {
        noc_hang(NocHangMethod::AccessNonExistantEndpoint);
    }

    fn eth_hang(method: EthHangMethod) {
        let chips = luwen_ref::detect_chips().unwrap();

        let mut expected_chip_count = 0;
        for chip in chips {
            if let Some(wh) = chip.as_wh() {
                if !wh.is_remote {
                    expected_chip_count += 1;
                    hang_eth(method.clone(), 0, wh).unwrap();
                }
            }
        }

        let chips = luwen_ref::detect_chips_fallible().unwrap();

        assert_eq!(
            chips.len(),
            expected_chip_count,
            "{:?}",
            chips.iter().map(|v| v.status()).collect::<Vec<_>>()
        );

        for chip in chips {
            if let Some(chip) = chip.status() {
                if !chip.eth_status.has_error() {
                    panic!("ARC should be hung but wasn't: {chip:?}");
                }
            } else {
                panic!("Found chip that still initialized after arc was hung");
            }
        }
    }

    #[test]
    #[ignore = "requires reset dongle"]
    fn overwrite_fw_eth_hang() {
        eth_hang(EthHangMethod::OverwriteEthFw);
    }

    #[test]
    #[ignore = "requires reset dongle"]
    fn overwrite_version_eth_hang() {
        eth_hang(EthHangMethod::OverwriteFwVersion);
    }
}
