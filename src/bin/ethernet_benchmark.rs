use luwen_if::{chip::HlComms, ChipImpl};
use luwen_ref::error::LuwenError;
use rand::Rng;

fn read_write_test(chip: impl HlComms, x: u8, y: u8, size: usize) -> (f64, f64) {
    let mut rng = rand::thread_rng();

    let mut write_data = Vec::with_capacity(size);
    for _ in 0..size {
        write_data.push(rng.gen());
    }

    let write_time = std::time::Instant::now();
    chip.noc_write(0, x, y, 0x0, &write_data);
    let write_time = write_time.elapsed().as_secs_f64();

    let mut readback_data = vec![0; size];

    let read_time = std::time::Instant::now();
    chip.noc_read(0, x, y, 0x0, &mut readback_data);
    let read_time = read_time.elapsed().as_secs_f64();

    for (index, (d, r)) in write_data
        .chunks(4)
        .zip(readback_data.chunks(4))
        .enumerate()
    {
        let d = u32::from_le_bytes([d[0], d[1], d[2], d[3]]);
        let r = u32::from_le_bytes([r[0], r[1], r[2], r[3]]);

        if d != r {
            for (index, r) in readback_data.chunks(4).enumerate() {
                let r = u32::from_le_bytes([r[0], r[1], r[2], r[3]]);

                if r == d {
                    println!("Match at {}", index)
                }
            }
            panic!("Data mismatch at {index} ({d:x} != {r:x})");
        }
    }

    (write_time, read_time)
}

pub fn main() -> Result<(), LuwenError> {
    let chips = luwen_ref::detect_chips()?;

    for (chip_index, chip) in chips.into_iter().enumerate() {
        let size = 1000 * 10_000;
        // let size = 1 << 19;
        // let size = 1000;
        let (write_time, read_time) = if let Some(wh) = chip.as_wh() {
            read_write_test(wh, 0, 0, size)
        } else if let Some(gs) = chip.as_gs() {
            read_write_test(gs, 1, 0, size)
        } else {
            unimplemented!("Chip of arch {:?} not supported", chip.get_arch());
        };

        println!(
            "Chip {} write: {:.1} MB/s, read: {:.1} MB/s",
            chip_index,
            (size as f64 / 1_048_576.0) / write_time,
            (size as f64 / 1_048_576.0) / read_time
        );
    }

    Ok(())
}
