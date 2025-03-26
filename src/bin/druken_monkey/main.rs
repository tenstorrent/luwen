use luwen_if::{
    chip::{Chip, HlComms, HlCommsInterface, InitStatus},
    error::{BtWrapper, PlatformError},
    ArcMsgError, ArcMsgProtocolError, ArcState, ChipImpl,
};
use luwen_ref::error::LuwenError;
use std::any::Any;
use std::panic;
use std::process::Command;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::{backtrace::Backtrace, time::Duration};

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
                msg: luwen_if::TypedArcMsg::SetArcState {
                    state: ArcState::A5,
                }
                .into(),
                ..Default::default()
            })?;
        }
        ArcHangMethod::CoreHault => {
            // Need to go into arc a3 before haulting the core, otherwise we can interrupt
            // communication with the voltage regulator.
            chip.arc_msg(luwen_if::chip::ArcMsgOptions {
                msg: luwen_if::TypedArcMsg::SetArcState {
                    state: ArcState::A3,
                }
                .into(),
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

#[allow(clippy::type_complexity)]
fn run_detect_test() -> Result<Option<Vec<(bool, Option<InitStatus>)>>, LuwenError> {
    let mut chip_details = Vec::new();
    let partial_chips = luwen_ref::detect_chips_fallible()?;

    //warm reset (internal)
    //reset board (external)

    for chip in partial_chips {
        let status = chip.status().cloned();
        if let Some(v) = chip.try_upgrade() {
            // let eth_status = chip.eth_safe();
            let remote = if let Some(wh) = v.as_wh() {
                wh.is_remote
            } else {
                false
            };
            chip_details.push((remote, status));
        }
    }

    if !chip_details.is_empty() {
        Ok(Some(chip_details))
    } else {
        Ok(None)
    }
}

fn compare_and_reset(expected: &dyn Any) {
    println!("Running detect test");
    if expected.is::<PlatformError>() {
        // Handling PlatformError
        let platform_error = expected.downcast_ref::<PlatformError>().unwrap();
        match run_detect_test() {
            Err(err) => {
                // Comparison with the expected error from the main function
                // let bt1 = err.to_string();
                // let bt2 = platform_error.to_string();
                // assert_eq!(bt1, bt2);
                println!("Actual: {:?}", err);
                println!("Expected: {:?}", platform_error);
            }
            _ => panic!("Expected error not found"),
        }
    } else if expected.is::<BtWrapper>() {
        // Handling Backtrace
        let backtrace = expected.downcast_ref::<BtWrapper>().unwrap();
        // Perform actions specific to Backtrace
        match run_detect_test() {
            Err(err) => {
                // Comparison with the expected error from the main function
                // let bt1 = err.to_string();
                // let bt2 = backtrace.to_string();
                // assert_eq!(bt1, bt2);
                println!("Actual: {:?}", err);
                println!("Expected: {:?}", backtrace);
            }
            _ => panic!("Expected error not found"),
        }
    } else if expected.is::<Vec<Chip>>() {
        // Unsupported type
        match run_detect_test() {
            Ok(Some(chip_details)) => {
                // Comparisons based on the returned value from run_detect_test
                if chip_details[0].1.is_some() {
                    println!("Chip is partially initialized");
                } else {
                    println!("Chip is fully initialized");
                }
            }
            _ => panic!("Expected error not found"),
        }
    }

    // Trigger the command in the terminal

    reset_board();
}

fn reset_board() {
    let _ = Command::new("/bin/bash")
        .arg("-c")
        .arg(
            r#"
            cd ~/work/syseng/src/t6ifc/t6py &&
            . bin/venv-activate.sh my-env &&
            reset-board
        "#,
        )
        .spawn();

    println!("waiting for ddr training to complete");
    let duration = Duration::from_secs(40);
    thread::sleep(duration);

    //delay time seconds

    println!("warm reset triggered");
}

fn main() {
    let commands = vec![
        "arc a5",
        "arc hault",
        "noc cg",
        "noc oob",
        "eth ver 1",
        "eth fw 1",
    ];
    // let commands = vec!["noc cg", "noc oob", "eth ver 1", "eth fw 1"];
    for cmd in commands {
        let args = cmd.split(' ').collect::<Vec<_>>();
        let command = args[0];
        let option = args.get(1);
        println!("Command: {} Option: {}", command, option.unwrap_or(&"None"));

        let mut chips = luwen_ref::detect_chips().unwrap();

        match (command, option) {
            ("arc", Some(opt)) => {
                let method = match *opt {
                    "overwrite" => ArcHangMethod::OverwriteFwCode,
                    "a5" => ArcHangMethod::A5,
                    "hault" => ArcHangMethod::CoreHault,
                    _ => unimplemented!("Have not yet implemented support for arc hang method"),
                };
                let duration = Duration::from_secs(1);
                let expected = PlatformError::ArcMsgError(ArcMsgError::ProtocolError {
                    source: ArcMsgProtocolError::Timeout(duration),
                    backtrace: BtWrapper(Backtrace::capture()),
                });

                hang_arc(method, chips.pop().unwrap()).unwrap();
                compare_and_reset(&expected);
            }
            ("noc", Some(opt)) => {
                let method = match *opt {
                    "cg" => NocHangMethod::AccessCgRow,
                    "oob" => NocHangMethod::AccessNonExistantEndpoint,
                    other => {
                        unimplemented!("Have not implemented support for noc hang method {other}")
                    }
                };

                // let expected = BtWrapper(Backtrace::capture());
                let chips_arc = Arc::new(Mutex::new(chips));
                let chips_clone = Arc::clone(&chips_arc);
                let handle = thread::spawn(move || {
                    let mut chips = chips_clone.lock().unwrap();
                    hang_noc(method, chips.pop().unwrap()).unwrap();
                });

                // Wait for the thread to finish and handle any panics
                let result = handle.join();
                match result {
                    Ok(_) => {
                        println!("Operation completed without a panic");
                        // Continue with the rest of your logic
                    }
                    Err(panic_info) => {
                        if let Some(s) = panic_info.downcast_ref::<&str>() {
                            println!("Panic occurred: {}", s);
                        } else {
                            println!("Panic occurred");
                        }
                        // Continue with the rest of your logic
                    }
                }
                // hang_noc(method, chips.pop().unwrap()).unwrap();
                // compare_and_reset(&expected);
                reset_board();
            }
            ("eth", Some(opt)) => {
                let method = match *opt {
                    "ver" => EthHangMethod::OverwriteFwVersion,
                    "fw" => EthHangMethod::OverwriteEthFw,
                    other => {
                        unimplemented!(
                            "Have not yet implementd support for eth hang method {other}"
                        );
                    }
                };

                let core = args.get(2).map(|v| v.parse()).unwrap_or(Ok(0)).unwrap();
                hang_eth(method, core, chips.pop().unwrap()).unwrap();
                compare_and_reset(&Some(chips.pop()));
            }
            _ => unimplemented!("Have not yet implemented support for this command"),
        }
    }
}
