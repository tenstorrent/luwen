use clap::{Parser, ValueEnum};
use drunken_monkey::hang::{arc::ArcHangMethod, eth::EthHangMethod, noc::NocHangMethod};

#[derive(ValueEnum, Debug, Clone)]
pub enum HangType {
    Noc(NocHangMethod),
    Eth(EthHangMethod),
    Arc(ArcHangMethod),
}

#[derive(Parser, Debug)]
pub struct Options {
    ty: HangType,
}
