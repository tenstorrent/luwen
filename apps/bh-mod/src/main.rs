#![warn(clippy::pedantic)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]

use std::path::PathBuf;

use anyhow::Context as _;
use clap::Parser as _;
use luwen::api::ChipImpl as _;
use luwen::pci::detect_chips;

mod reset;
mod table;

fn main() -> anyhow::Result<()> {
    run()
}

fn run() -> anyhow::Result<()> {
    let args = Args::parse();
    let chips = detect_chips().map_err(|e| anyhow::anyhow!("{e}"))?;
    anyhow::ensure!(!chips.is_empty(), "no chips detected");
    let filter: Vec<u32> = args
        .dev
        .iter()
        .filter_map(|p| p.file_name()?.to_str()?.parse().ok())
        .collect();
    for (idx, chip) in chips.iter().enumerate() {
        let info = chip
            .get_device_info()
            .map_err(|e| anyhow::anyhow!("{e}"))?
            .context("no device info")?;
        let interface_id = info.interface_id as usize;
        if !filter.is_empty() && !filter.contains(&info.interface_id) {
            continue;
        }
        let bh = chip
            .as_bh()
            .with_context(|| format!("device {idx} is not a Blackhole chip"))?;
        let wrote = match &args.cmd {
            Cmd::Get { table, fmt, fields } => {
                table::get(bh, table.as_ref(), fmt, fields)?;
                false
            }
            Cmd::Set { dry_run, fields } => {
                let mut op = table::Set::new(bh);
                for f in fields {
                    op = op.field(f);
                }
                if *dry_run {
                    op = op.dry_run();
                }
                op.run()?;
                !dry_run
            }
            Cmd::Res {
                dry_run,
                all,
                fields,
            } => {
                let mut op = table::Reset::new(bh);
                if !all {
                    for f in fields {
                        op = op.field(f);
                    }
                }
                if *dry_run {
                    op = op.dry_run();
                }
                op.run()?;
                !dry_run
            }
        };
        if wrote {
            reset::chip_reset(interface_id)
                .map_err(|e| anyhow::anyhow!("{e}"))
                .context("chip reset failed")?;
        }
    }
    Ok(())
}

/// Read and modify Blackhole SPI flash protobuf tables.
///
/// Any write operation (set, res) performs a chip reset to activate changes.
#[derive(clap::Parser)]
struct Args {
    /// Path under /dev/tenstorrent to operate on. Repeatable. Omit to target all available devices.
    #[arg(short = 'd', long = "dev", value_name = "PATH", global = true)]
    dev: Vec<PathBuf>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(clap::Subcommand)]
enum Cmd {
    /// Print a table.
    Get {
        /// Table to print; defaults to fw-table.
        #[arg(short = 't', long)]
        table: Option<Table>,
        /// Output format.
        #[arg(
            short = 'f',
            long = "fmt",
            visible_alias = "format",
            default_value = "pretty"
        )]
        fmt: Fmt,
        /// Fields to include (dot-notation path); omit to include all.
        fields: Vec<String>,
    },
    /// Write fields to fw_table.
    #[command(visible_alias = "s")]
    Set {
        /// Print what would change without writing to flash or resetting.
        #[arg(short = 'n', long)]
        dry_run: bool,
        /// Field assignments in `field=value` form (dot-notation path).
        fields: Vec<String>,
    },
    /// Restore fields from the factory default (origcfg).
    #[command(visible_aliases = ["r", "reset"])]
    Res {
        /// Print what would change without writing to flash or resetting.
        #[arg(short = 'n', long)]
        dry_run: bool,
        /// Restore all fields to factory default.
        #[arg(short = 'a', long, conflicts_with = "fields")]
        all: bool,
        /// Fields to restore (dot-notation path); conflicts with --all.
        fields: Vec<String>,
    },
}

/// A protobuf table in SPI flash.
#[derive(Clone)]
#[derive(clap::ValueEnum)]
pub enum Table {
    /// Writable firmware config table (`cmfwcfg`).
    FwTable,
    /// Read-only board config table (`boardcfg`).
    ReadOnly,
    /// Read-only flash info table (`flshinfo`).
    FlashInfo,
}

/// Output format for the `get` subcommand.
#[derive(Clone)]
#[derive(clap::ValueEnum)]
pub enum Fmt {
    /// ASCII table.
    Pretty,
    /// JSON.
    Json,
}
