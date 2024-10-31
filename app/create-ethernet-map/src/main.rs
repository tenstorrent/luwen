use clap::Parser;

#[derive(Parser)]
pub struct CmdArgs {
    file: String,
}

fn main() -> Result<(), luwen_ref::error::LuwenError> {
    let args = CmdArgs::parse();

    create_ethernet_map::generate_map(args.file)
}
