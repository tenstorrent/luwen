use luwen::pci::detect_chips;
use std::fs::{self, File};
use std::io::{self, Write};

const FLASH_DUMP_SIZE: usize = 16 * 1024 * 1024; // 16 MB
const CHUNK_SIZE: usize = 64 * 1024; // Read in 64KB chunks
const OUTPUT_DIR: &str = "flash_dump";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let devices = detect_chips()?;

    if devices.is_empty() {
        eprintln!("No chips detected");
        return Err("No chips found".into());
    }

    // Create output directory
    fs::create_dir_all(OUTPUT_DIR)?;
    eprintln!("Created output directory: {}", OUTPUT_DIR);

    let mut stderr = io::stderr();
    let mut device_count = 0;

    // Process each detected device
    for (index, device) in devices.iter().enumerate() {
        // Get device identifier for filename
        let device_id = device
            .inner
            .get_device_info()
            .map_err(|e| format!("Failed to get device info: {}", e))?
            .map(|info| info.interface_id.to_string())
            .unwrap_or_else(|| format!("device_{}", index));

        // Determine chip type and create appropriate filename
        let (chip_type, filename) = if let Some(_bh) = device.as_bh() {
            let filename = format!("{}/blackhole_{}.bin", OUTPUT_DIR, device_id);
            ("Blackhole", filename)
        } else if let Some(_wh) = device.as_wh() {
            let filename = format!("{}/wormhole_{}.bin", OUTPUT_DIR, device_id);
            ("Wormhole", filename)
        } else {
            writeln!(stderr, "Skipping unsupported chip type at index {}", index)?;
            continue;
        };

        writeln!(
            stderr,
            "Processing {} chip (device {}) - output: {}",
            chip_type, device_id, filename
        )?;

        // Create file and dump flash
        let file = File::create(&filename)?;
        let mut file_writer = io::BufWriter::new(file);

        if let Some(bh) = device.as_bh() {
            dump_flash_bh(bh, 0, &mut file_writer)?;
        } else if let Some(wh) = device.as_wh() {
            dump_flash_wh(wh, 0, &mut file_writer)?;
        }

        writeln!(stderr, "Completed dump for {}: {}", chip_type, filename)?;
        device_count += 1;
    }

    writeln!(
        stderr,
        "\nFlash dump complete: {} device(s) processed, files saved to {}",
        device_count, OUTPUT_DIR
    )?;
    Ok(())
}

fn dump_flash_bh<W: Write>(
    chip: &luwen::api::chip::Blackhole,
    start_addr: u32,
    writer: &mut W,
) -> Result<(), Box<dyn std::error::Error>> {
    dump_flash_impl(|addr, buf| chip.spi_read(addr, buf), start_addr, writer)
}

fn dump_flash_wh<W: Write>(
    chip: &luwen::api::chip::Wormhole,
    start_addr: u32,
    writer: &mut W,
) -> Result<(), Box<dyn std::error::Error>> {
    dump_flash_impl(|addr, buf| chip.spi_read(addr, buf), start_addr, writer)
}

fn dump_flash_impl<F, W>(
    spi_read: F,
    start_addr: u32,
    writer: &mut W,
) -> Result<(), Box<dyn std::error::Error>>
where
    F: Fn(u32, &mut [u8]) -> Result<(), Box<dyn std::error::Error>>,
    W: Write,
{
    let mut stderr = io::stderr();
    let mut buffer = vec![0u8; CHUNK_SIZE];
    let mut total_read = 0;

    while total_read < FLASH_DUMP_SIZE {
        let remaining = FLASH_DUMP_SIZE - total_read;
        let read_size = remaining.min(CHUNK_SIZE);
        let current_addr = start_addr + total_read as u32;

        // Resize buffer if needed for the last chunk
        if read_size < CHUNK_SIZE {
            buffer.resize(read_size, 0);
        }

        // Read chunk from flash
        spi_read(current_addr, &mut buffer[..read_size])?;

        // Write raw binary data to file
        writer.write_all(&buffer[..read_size])?;

        total_read += read_size;

        // Progress indicator to stderr
        let progress = (total_read * 100) / FLASH_DUMP_SIZE;
        if total_read % (1024 * 1024) == 0 || total_read == FLASH_DUMP_SIZE {
            writeln!(
                stderr,
                "  Progress: {} / {} bytes ({}%)",
                total_read, FLASH_DUMP_SIZE, progress
            )?;
        }
    }

    writer.flush()?;
    Ok(())
}

