use clap::Parser;
use std::io;
use std::fs::File;

#[derive(Parser)]
pub struct CmdArgs {
    #[arg(short, long)]
    output: Option<String>,

    #[arg(short, long)]
    check : Option<String>,
}

fn main() -> Result<(), luwen_ref::error::LuwenError> {
    let args = CmdArgs::parse();

    let map = create_ethernet_map::generate_map()?;
    match &args.output.as_deref() {
        Some("-") => {
            let stdout = io::stdout();
            let handle = stdout.lock();
            create_ethernet_map::write_ethernet_map(handle, &map)
        }
        Some(filename) => {
            let output_file = File::create(filename)
                .map_err(|e| luwen_ref::error::LuwenError::Custom(format!("Failed to create file: {e}")))?;
            create_ethernet_map::write_ethernet_map(output_file, &map)
        }
        None => Ok(())
    };
    match &args.check {
        Some(filename) => {
            let check_map = create_ethernet_map::read_ethernet_map(filename)?;
            if map == check_map {
                return Ok(());
            } else {
                return Err(luwen_ref::error::LuwenError::Custom("Maps differ".into()));
            }
        }
        None => Ok(())
    }
}
