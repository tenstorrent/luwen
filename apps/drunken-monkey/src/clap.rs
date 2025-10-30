use clap::{Parser, Subcommand};
use drunken_monkey::hang::{arc::ArcHangMethod, eth::EthHangMethod, noc::NocHangMethod};

#[derive(Subcommand, Debug, Clone)]
pub enum HangType {
    Noc {
        #[arg(value_enum)]
        method: NocHangMethod,
    },
    Eth {
        #[arg(value_enum)]
        method: EthHangMethod,
        eth_id: u32,
    },
    Arc {
        #[arg(value_enum)]
        method: ArcHangMethod,
    },
}

#[derive(Parser, Debug)]
pub struct CliOptions {
    #[command(subcommand)]
    pub ty: HangType,
}
