use clap::Parser;

#[derive(Parser)]
pub struct CmdArgs {
    file: String,
}

fn main() -> Result<(), luwen::pcie::error::LuwenError> {
    let args = CmdArgs::parse();

    create_ethernet_map::generate_map(args.file)
}
