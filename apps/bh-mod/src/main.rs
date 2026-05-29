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
    let mut selected: Vec<(usize, &luwen_api::chip::Blackhole)> = Vec::new();
    for (idx, chip) in chips.iter().enumerate() {
        let info = chip
            .get_device_info()
            .map_err(|e| anyhow::anyhow!("{e}"))?
            .context("no device info")?;
        if !filter.is_empty() && !filter.contains(&info.interface_id) {
            continue;
        }
        let bh = chip
            .as_bh()
            .with_context(|| format!("device {idx} is not a Blackhole chip"))?;
        selected.push((info.interface_id as usize, bh));
    }
    if selected.is_empty() {
        return Ok(());
    }
    let wrote = match &args.cmd {
        Cmd::Get {
            table,
            fmt,
            delta,
            fields,
        } => {
            table::get(&selected, table.as_ref(), fmt, *delta, fields)?;
            false
        }
        Cmd::Set { dry_run, fields } => {
            let mut op = table::Set::new(&selected);
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
            let mut op = table::Reset::new(&selected);
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
        for (interface_id, _) in &selected {
            reset::chip_reset(*interface_id)
                .map_err(|e| anyhow::anyhow!("{e}"))
                .context("chip reset")?;
        }
    }
    Ok(())
}

/// Read and modify Blackhole SPI flash firmware configuration.
///
/// `set` and `res` operate on the `ccfgovr` override banks, which the
/// firmware merges on top of `cmfwcfg` at boot. The original `cmfwcfg`
/// partition is never written. Any write operation performs a chip reset
/// so the new config takes effect.
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
        /// Only show rows whose override is set.
        #[arg(long)]
        delta: bool,
        /// Fields to include (dot-notation path); omit to include all.
        fields: Vec<String>,
    },
    /// Merge fields into the `ccfgovr` override.
    #[command(visible_alias = "s")]
    Set {
        /// Print what would change without writing to flash or resetting.
        #[arg(short = 'n', long)]
        dry_run: bool,
        /// Field assignments in `field=value` form (dot-notation path).
        fields: Vec<String>,
    },
    /// Remove fields from the `ccfgovr` override (cmfwcfg value re-emerges).
    #[command(visible_aliases = ["r", "reset"])]
    Res {
        /// Print what would change without writing to flash or resetting.
        #[arg(short = 'n', long)]
        dry_run: bool,
        /// Clear all override fields.
        #[arg(short = 'a', long, conflicts_with = "fields")]
        all: bool,
        /// Fields to remove from the override (dot-notation path);
        /// conflicts with --all.
        fields: Vec<String>,
    },
}

/// A protobuf table in SPI flash.
#[derive(Clone, clap::ValueEnum)]
pub enum Table {
    /// Firmware config view (`cmfwcfg` defaults alongside the active
    /// `ccfgovr` override).
    FwTable,
    /// Read-only board config table (`boardcfg`).
    ReadOnly,
    /// Read-only flash info table (`flshinfo`).
    FlashInfo,
}

/// Output format for the `get` subcommand.
#[derive(Clone, clap::ValueEnum)]
pub enum Fmt {
    /// ASCII table.
    Pretty,
    /// JSON.
    Json,
}
