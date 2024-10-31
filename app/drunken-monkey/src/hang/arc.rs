use clap::ValueEnum;

use luwen_if::{
    chip::{ArcMsgOptions, Chip, HlCommsInterface},
    ArcState, ChipImpl, TypedArcMsg,
};

#[derive(Debug, Clone, ValueEnum)]
pub enum ArcHangMethod {
    OverwriteFwCode,
    A5,
    CoreHault,
}

pub fn hang_arc(method: ArcHangMethod, chip: Chip) -> Result<(), Box<dyn std::error::Error>> {
    match method {
        ArcHangMethod::OverwriteFwCode => {
            unimplemented!("Haven't implemented fw overrwrite");
        }
        ArcHangMethod::A5 => {
            chip.arc_msg(ArcMsgOptions {
                msg: TypedArcMsg::SetArcState {
                    state: ArcState::A5,
                }
                .into(),
                ..Default::default()
            })?;
        }
        ArcHangMethod::CoreHault => {
            // Need to go into arc a3 before haulting the core, otherwise we can interrupt
            // communication with the voltage regulator.
            chip.arc_msg(ArcMsgOptions {
                msg: TypedArcMsg::SetArcState {
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
