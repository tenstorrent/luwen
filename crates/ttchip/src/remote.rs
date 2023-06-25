pub mod detect;

use std::fmt::Display;

use crate::Wormhole;
use kmdif::PciError;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct EthCoord {
    rack_x: u8,
    rack_y: u8,
    shelf_x: u8,
    shelf_y: u8,
}

pub trait IntoChip<T>: Sized {
    fn cinto(&self, chip: &mut Wormhole) -> Result<T, PciError>;
}

pub fn get_local_chip_coord(chip: &mut Wormhole) -> Result<EthCoord, PciError> {
    let coord = chip.noc(false).read32(9, 0, 0x1108)?;

    Ok(EthCoord {
        rack_x: (coord & 0xFF) as u8,
        rack_y: ((coord >> 8) & 0xFF) as u8,
        shelf_x: ((coord >> 16) & 0xFF) as u8,
        shelf_y: ((coord >> 24) & 0xFF) as u8,
    })
}

impl IntoChip<EthCoord> for EthCoord {
    fn cinto(&self, _chip: &mut Wormhole) -> Result<EthCoord, PciError> {
        Ok(self.clone())
    }
}

impl IntoChip<EthCoord> for (Option<u8>, Option<u8>, Option<u8>, Option<u8>) {
    fn cinto(&self, chip: &mut Wormhole) -> Result<EthCoord, PciError> {
        let local_coord = get_local_chip_coord(chip)?;

        let rack_x = self.0.unwrap_or_else(|| local_coord.rack_x);
        let rack_y = self.1.unwrap_or_else(|| local_coord.rack_y);
        let shelf_x = self.2.unwrap_or_else(|| local_coord.shelf_x);
        let shelf_y = self.3.unwrap_or_else(|| local_coord.shelf_y);

        Ok(EthCoord {
            rack_x,
            rack_y,
            shelf_x,
            shelf_y,
        })
    }
}

impl IntoChip<EthCoord> for (u8, u8, u8, u8) {
    fn cinto(&self, _chip: &mut Wormhole) -> Result<EthCoord, PciError> {
        let (rack_x, rack_y, shelf_x, shelf_y) = *self;

        Ok(EthCoord {
            rack_x,
            rack_y,
            shelf_x,
            shelf_y,
        })
    }
}

impl IntoChip<EthCoord> for (u8, u8) {
    fn cinto(&self, chip: &mut Wormhole) -> Result<EthCoord, PciError> {
        (None, None, Some(self.0), Some(self.1)).cinto(chip)
    }
}

impl Display for EthCoord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "[{rack_x}, {rack_y}, {shelf_x}, {self_y}]",
            rack_x = self.rack_x,
            rack_y = self.rack_y,
            shelf_x = self.shelf_x,
            self_y = self.shelf_y
        ))
    }
}

#[derive(Clone)]
pub struct EthCommCoord {
    coord: EthCoord,
    noc_id: u8,
    noc_x: u8,
    noc_y: u8,
    offset: u64,
}

pub fn get_rack_addr(coord: &EthCommCoord) -> u32 {
    ((coord.coord.rack_y as u32) << 8) | (coord.coord.rack_x as u32)
}

pub fn get_sys_addr(coord: &EthCommCoord) -> u64 {
    let mut addr = coord.coord.shelf_y as u64;
    addr = (addr << 6) | (coord.coord.shelf_x as u64);
    addr = (addr << 6) | (coord.noc_y as u64);
    addr = (addr << 6) | (coord.noc_x as u64);
    addr = (addr << 36) | (coord.offset as u64);

    addr
}

const MAX_BLOCK: u32 = 1024;

const Q_SIZE: u32 = 192;
const Q_SIZE_WORDS: u32 = 48;
const Q_ENTRY_WORDS: u32 = 8;
const Q_ENTRY_BYTES: u32 = 32;

const REMOTE_UPDATE_PTR_SIZE_BYTES: u32 = 16;
const CMD_BUF_SIZE: u32 = 4;
const CMD_BUF_SIZE_MASK: u32 = 0x3;

const CMD_WR_REQ: u32 = 0x1;
const CMD_RD_REQ: u32 = 0x4;
const CMD_RD_DATA: u32 = 0x8;

const CMD_DATA_BLOCK_DRAM: u32 = 0x1 << 4;
const CMD_LAST_DATA_BLOCK_DRAM: u32 = 0x1 << 5;
const CMD_DATA_BLOCK: u32 = 0x1 << 6;
const NOC_ID_SHIFT: u32 = 9;
const NOC_ID_MASK: u32 = 0x1;
const NOC_ID_SEL: u32 = NOC_ID_MASK << NOC_ID_SHIFT;
const CMD_TIMESTAMP_SHIFT: u32 = 10;
const CMD_TIMESTAMP: u32 = 0x1 << CMD_TIMESTAMP_SHIFT;
const CMD_DATA_BLOCK_UNAVAILABLE: u32 = 0x1 << 30;
const CMD_DEST_UNREACHABLE: u32 = 0x1 << 31;

const REQ_Q_ADDR: u32 = 0x80;
const ETH_IN_REQ_Q: u32 = REQ_Q_ADDR + Q_SIZE;
const RESP_Q_ADDR: u32 = REQ_Q_ADDR + 2 * Q_SIZE;
const ETH_OUT_REQ_Q: u32 = REQ_Q_ADDR + 3 * Q_SIZE;

const WR_PTR_OFFSET: u32 = 0 + 8;
const RD_PTR_OFFSET: u32 = 4 + 8;
const CMD_OFFSET: u32 = 8 + 8;
const ADDR_L_OFFSET: u32 = 0;
const ADDR_H_OFFSET: u32 = 1;
const DATA_OFFSET: u32 = 2;
const FLAGS_OFFSET: u32 = 3;
const SRC_RESP_BUF_INDEX_OFFSET: u32 = 4;
const LCL_BUF_INDEX_OFFSET: u32 = 5;
const SRC_RESP_Q_ID_OFFSET: u32 = 6;
const SRC_ADDR_TAG_OFFSET: u32 = 7;

fn wait_for_idle(
    chip: &mut Wormhole,
    command_q_addr: u32,
    eth_x: u8,
    eth_y: u8,
    timeout: std::time::Duration,
) -> Result<u32, PciError> {
    let is_cmd_q_full = |chip: &mut Wormhole, curr_wptr: u32| {
        let curr_rptr = chip.noc(false).read32(
            eth_x,
            eth_y,
            command_q_addr + REQ_Q_ADDR + 4 * RD_PTR_OFFSET,
        )?;

        Ok((curr_wptr != curr_rptr)
            && ((curr_wptr & CMD_BUF_SIZE_MASK) == (curr_rptr & CMD_BUF_SIZE_MASK)))
    };

    let mut curr_wptr = chip.noc(false).read32(
        eth_x,
        eth_y,
        command_q_addr + REQ_Q_ADDR + 4 * WR_PTR_OFFSET,
    )?;

    let start = std::time::Instant::now();
    while is_cmd_q_full(chip, curr_wptr)? {
        if start.elapsed() > timeout {
            panic!("TIMEOUT")
        }
        curr_wptr = chip.noc(false).read32(
            eth_x,
            eth_y,
            command_q_addr + REQ_Q_ADDR + 4 * WR_PTR_OFFSET,
        )?;
    }

    Ok(curr_wptr)
}

pub fn read32(
    chip: &mut Wormhole,
    command_q_addr: u32,
    coord: EthCommCoord,
    eth_x: u8,
    eth_y: u8,
    timeout: std::time::Duration,
) -> Result<u32, PciError> {
    let curr_wptr = wait_for_idle(chip, command_q_addr, eth_x, eth_y, timeout)?;

    let cmd_addr =
        command_q_addr + REQ_Q_ADDR + 4 * CMD_OFFSET + (curr_wptr % CMD_BUF_SIZE) * Q_ENTRY_BYTES;

    let sys_addr = get_sys_addr(&coord);
    let rack_addr = get_rack_addr(&coord);

    chip.noc(false)
        .write32(eth_x, eth_y, cmd_addr, (sys_addr & 0xFFFFFFFF) as u32)?;
    chip.noc(false)
        .write32(eth_x, eth_y, cmd_addr + 4, (sys_addr >> 32) as u32)?;
    chip.noc(false)
        .write32(eth_x, eth_y, cmd_addr + 16, (rack_addr & 0xFFFF) as u32)?;

    let mut flags = CMD_RD_REQ;
    flags |= (((coord.noc_id as u32) & NOC_ID_MASK) as u32) << NOC_ID_SHIFT;
    chip.noc(false)
        .write32(eth_x, eth_y, cmd_addr + 12, flags)?;

    let next_wptr = (curr_wptr + 1) % (2 * CMD_BUF_SIZE);
    chip.noc(false).write32(
        eth_x,
        eth_y,
        command_q_addr + REQ_Q_ADDR + 4 * WR_PTR_OFFSET,
        next_wptr,
    )?;

    let curr_rptr = chip.noc(false).read32(
        eth_x,
        eth_y,
        command_q_addr + RESP_Q_ADDR + 4 * RD_PTR_OFFSET,
    )?;
    let mut curr_wptr = chip.noc(false).read32(
        eth_x,
        eth_y,
        command_q_addr + RESP_Q_ADDR + 4 * WR_PTR_OFFSET,
    )?;

    let start_time = std::time::Instant::now();
    while curr_wptr == curr_rptr {
        curr_wptr = chip.noc(false).read32(
            eth_x,
            eth_y,
            command_q_addr + RESP_Q_ADDR + 4 * WR_PTR_OFFSET,
        )?;
        if start_time.elapsed() > timeout {
            panic!("TIMEOUT");
        }
    }

    let cmd_addr =
        command_q_addr + RESP_Q_ADDR + 4 * CMD_OFFSET + (curr_rptr % CMD_BUF_SIZE) * Q_ENTRY_BYTES;

    let mut flags = 0;
    let start_time = std::time::Instant::now();
    while flags == 0 {
        flags = chip.noc(false).read32(eth_x, eth_y, cmd_addr + 12)?;
        if start_time.elapsed() > timeout {
            panic!("TIMEOUT");
        }
    }

    let is_block = (flags & CMD_DATA_BLOCK) == 64;
    let data = chip.noc(false).read32(eth_x, eth_y, cmd_addr + 8)?;

    let mut error = None;
    if flags & CMD_DEST_UNREACHABLE != 0 {
        error = Some("Destination Unreachable.");
    }
    if flags & CMD_DATA_BLOCK_UNAVAILABLE != 0 {
        error = Some("Unable to reserve data block on destination route.");
    }

    let mut flag_block_read = false;
    if is_block {
        if flags & CMD_RD_DATA != 0 {
            flag_block_read = true;
        }
    }

    if flag_block_read {
        error = Some("Found block read response expected something else");
    }

    let next_rptr = (curr_rptr + 1) % (2 * CMD_BUF_SIZE);
    chip.noc(false).write32(
        eth_x,
        eth_y,
        command_q_addr + RESP_Q_ADDR + 4 * RD_PTR_OFFSET,
        next_rptr,
    )?;

    if let Some(error) = error {
        panic!("{}", error);
    }

    Ok(data)
}

pub fn block_read(
    chip: &mut Wormhole,
    command_q_addr: u32,
    eth_x: u8,
    eth_y: u8,
    timeout: std::time::Duration,
    fake_it: bool,
    mut coord: EthCommCoord,
    data: &mut [u8],
) -> Result<(), PciError> {
    if fake_it {
        assert_eq!(data.len() % 4, 0);

        let data = unsafe { std::mem::transmute::<_, &mut [u32]>(data) };
        for d in data {
            *d = read32(chip, command_q_addr, coord.clone(), eth_x, eth_y, timeout)?;
            coord.offset += 4;
        }

        return Ok(());
    }

    let rack_addr = get_rack_addr(&coord);

    let buffer = chip.get_eth_dma_buffer()?;
    let mut buffer_pos = 0;

    let number_of_slices = 4;
    let buffer_slice_len = buffer.size / number_of_slices;
    let buffer_size = buffer.size;
    while buffer_pos < buffer_size {
        let sys_addr = get_sys_addr(&coord);

        let curr_wptr = wait_for_idle(chip, command_q_addr, eth_x, eth_y, timeout)?;

        let cmd_addr = command_q_addr
            + REQ_Q_ADDR
            + 4 * CMD_OFFSET
            + (curr_wptr % CMD_BUF_SIZE) * Q_ENTRY_BYTES;

        let dma_offset = buffer_slice_len * (curr_wptr as u64 % number_of_slices);

        let buffer = chip.get_eth_dma_buffer()?;

        let dma_phys_pointer = buffer.physical_address + dma_offset;
        let block_len = (data.len() as u64 - buffer_pos).min(buffer_slice_len) as usize;

        chip.noc(false)
            .write32(eth_x, eth_y, cmd_addr, (sys_addr & 0xFFFFFFFF) as u32)?;
        chip.noc(false)
            .write32(eth_x, eth_y, cmd_addr + 4, (sys_addr >> 32) as u32)?;
        chip.noc(false)
            .write32(eth_x, eth_y, cmd_addr + 16, (rack_addr & 0xFFFF) as u32)?;

        let mut flags = CMD_RD_REQ;
        flags |= (coord.noc_id as u32 & NOC_ID_MASK) << NOC_ID_SHIFT;
        flags |= CMD_DATA_BLOCK | CMD_DATA_BLOCK_DRAM;
        chip.noc(false)
            .write32(eth_x, eth_y, cmd_addr + 8, block_len as u32)?;
        chip.noc(false)
            .write32(eth_x, eth_y, cmd_addr + 28, dma_phys_pointer as u32)?;
        chip.noc(false)
            .write32(eth_x, eth_y, cmd_addr + 12, flags)?;

        let next_wptr = (curr_wptr + 1) % (2 * CMD_BUF_SIZE);
        chip.noc(false).write32(
            eth_x,
            eth_y,
            command_q_addr + REQ_Q_ADDR + 4 * WR_PTR_OFFSET,
            next_wptr,
        )?;

        let curr_rptr = chip.noc(false).read32(
            eth_x,
            eth_y,
            command_q_addr + RESP_Q_ADDR + 4 * RD_PTR_OFFSET,
        )?;
        let mut curr_wptr = chip.noc(false).read32(
            eth_x,
            eth_y,
            command_q_addr + RESP_Q_ADDR + 4 * WR_PTR_OFFSET,
        )?;

        let start_time = std::time::Instant::now();
        while curr_wptr == curr_rptr {
            curr_wptr = chip.noc(false).read32(
                eth_x,
                eth_y,
                command_q_addr + RESP_Q_ADDR + 4 * WR_PTR_OFFSET,
            )?;
            if start_time.elapsed() > timeout {
                panic!("TIMEOUT");
            }
        }

        let cmd_addr = command_q_addr
            + RESP_Q_ADDR
            + 4 * CMD_OFFSET
            + (curr_rptr % CMD_BUF_SIZE) * Q_ENTRY_BYTES;

        let mut flags = 0;
        let start_time = std::time::Instant::now();
        while flags == 0 {
            flags = chip.noc(false).read32(eth_x, eth_y, cmd_addr + 12)?;
            if start_time.elapsed() > timeout {
                panic!("TIMEOUT");
            }
        }

        let is_block = (flags & CMD_DATA_BLOCK) == 64;

        if flags & CMD_DEST_UNREACHABLE != 0 {
            panic!("Destination Unreachable.")
        }
        if flags & CMD_DATA_BLOCK_UNAVAILABLE != 0 {
            panic!("Unable to reserve data block on destination route.")
        }

        let mut flag_block_read = false;
        if is_block {
            if flags & CMD_RD_DATA != 0 {
                flag_block_read = true;
            }
        }

        if !flag_block_read {
            panic!("Found non block read response expected something else")
        }

        let next_rptr = (curr_rptr + 1) % (2 * CMD_BUF_SIZE);
        chip.noc(false).write32(
            eth_x,
            eth_y,
            command_q_addr + RESP_Q_ADDR + 4 * RD_PTR_OFFSET,
            next_rptr,
        )?;

        let buffer = chip.get_eth_dma_buffer()?;

        data[buffer_pos as usize..][..block_len]
            .copy_from_slice(&buffer.buffer[dma_offset as usize..][..block_len]);

        buffer_pos += buffer_slice_len;
        coord.offset += buffer_slice_len;
    }

    Ok(())
}

pub fn write32(
    chip: &mut Wormhole,
    command_q_addr: u32,
    coord: EthCommCoord,
    eth_x: u8,
    eth_y: u8,
    timeout: std::time::Duration,
    value: u32,
) -> Result<(), PciError> {
    let curr_wptr = wait_for_idle(chip, command_q_addr, eth_x, eth_y, timeout)?;

    let cmd_addr =
        command_q_addr + REQ_Q_ADDR + 4 * CMD_OFFSET + (curr_wptr % CMD_BUF_SIZE) * Q_ENTRY_BYTES;

    let sys_addr = get_sys_addr(&coord);
    let rack_addr = get_rack_addr(&coord);

    chip.noc(false)
        .write32(eth_x, eth_y, cmd_addr, (sys_addr & 0xFFFFFFFF) as u32)?;
    chip.noc(false)
        .write32(eth_x, eth_y, cmd_addr + 4, (sys_addr >> 32) as u32)?;
    chip.noc(false)
        .write32(eth_x, eth_y, cmd_addr + 16, (rack_addr & 0xFFFF) as u32)?;

    let mut flags = CMD_WR_REQ;
    flags |= ((coord.noc_id as u32) & NOC_ID_MASK) << NOC_ID_SHIFT;
    chip.noc(false).write32(eth_x, eth_y, cmd_addr + 8, value)?;
    chip.noc(false)
        .write32(eth_x, eth_y, cmd_addr + 12, flags)?;

    let next_wptr = (curr_wptr + 1) % (2 * CMD_BUF_SIZE);
    chip.noc(false).write32(
        eth_x,
        eth_y,
        command_q_addr + REQ_Q_ADDR + 4 * WR_PTR_OFFSET,
        next_wptr,
    )?;

    Ok(())
}

pub fn block_write(
    chip: &mut Wormhole,
    command_q_addr: u32,
    eth_x: u8,
    eth_y: u8,
    timeout: std::time::Duration,
    fake_it: bool,
    mut coord: EthCommCoord,
    data: &[u8],
) -> Result<(), PciError> {
    if fake_it {
        assert_eq!(data.len() % 4, 0);

        let data = unsafe { std::mem::transmute::<_, &[u32]>(data) };
        for d in data {
            write32(
                chip,
                command_q_addr,
                coord.clone(),
                eth_x,
                eth_y,
                timeout,
                *d,
            )?;
            coord.offset += 4;
        }

        return Ok(());
    }

    let rack_addr = get_rack_addr(&coord);

    let buffer = chip.get_eth_dma_buffer()?;
    let mut buffer_pos = 0;

    let number_of_slices = 4;
    let buffer_slice_len = buffer.size / number_of_slices;
    let buffer_size = buffer.size;
    while buffer_pos < buffer_size {
        let sys_addr = get_sys_addr(&coord);

        let curr_wptr = wait_for_idle(chip, command_q_addr, eth_x, eth_y, timeout)?;

        let cmd_addr = command_q_addr
            + REQ_Q_ADDR
            + 4 * CMD_OFFSET
            + (curr_wptr % CMD_BUF_SIZE) * Q_ENTRY_BYTES;

        let dma_offset = buffer_slice_len * (curr_wptr as u64 % number_of_slices);

        let buffer = chip.get_eth_dma_buffer()?;

        let dma_phys_pointer = buffer.physical_address + dma_offset;
        let block_len = (data.len() as u64 - buffer_pos).min(buffer_slice_len) as usize;

        buffer.buffer[dma_offset as usize..][..block_len]
            .copy_from_slice(&data[buffer_pos as usize..][..block_len]);

        chip.noc(false)
            .write32(eth_x, eth_y, cmd_addr, (sys_addr & 0xFFFFFFFF) as u32)?;
        chip.noc(false)
            .write32(eth_x, eth_y, cmd_addr + 4, (sys_addr >> 32) as u32)?;
        chip.noc(false)
            .write32(eth_x, eth_y, cmd_addr + 16, (rack_addr & 0xFFFF) as u32)?;

        let mut flags = CMD_WR_REQ;
        flags |= ((coord.noc_id as u32) & NOC_ID_MASK) << NOC_ID_SHIFT;
        flags |= CMD_DATA_BLOCK | CMD_DATA_BLOCK_DRAM;
        chip.noc(false)
            .write32(eth_x, eth_y, cmd_addr + 8, block_len as u32)?;
        chip.noc(false)
            .write32(eth_x, eth_y, cmd_addr + 28, dma_phys_pointer as u32)?;
        chip.noc(false)
            .write32(eth_x, eth_y, cmd_addr + 12, flags)?;

        let next_wptr = (curr_wptr + 1) % (2 * CMD_BUF_SIZE);
        chip.noc(false).write32(
            eth_x,
            eth_y,
            command_q_addr + REQ_Q_ADDR + 4 * WR_PTR_OFFSET,
            next_wptr,
        )?;

        buffer_pos += buffer_slice_len;
        coord.offset += buffer_slice_len;
    }

    Ok(())
}

fn read_heartbeat(chip: &mut Wormhole) -> Result<Vec<u32>, PciError> {
    const HEARTBEAT_ADDR: u32 = 0x1C;
    let eth_locations = [
        (9, 0),
        (1, 0),
        (8, 0),
        (2, 0),
        (7, 0),
        (3, 0),
        (6, 0),
        (4, 0),
        (9, 6),
        (1, 6),
        (8, 6),
        (2, 6),
        (7, 6),
        (3, 6),
        (6, 6),
        (4, 6),
    ];

    let mut output = Vec::with_capacity(eth_locations.len());
    for loc in eth_locations {
        output.push(chip.noc(false).read32(loc.0, loc.1, HEARTBEAT_ADDR)?)
    }

    Ok(output)
}

fn wait_for_training_to_complete(
    chip: &mut Wormhole,
    coord: EthCoord,
    timeout: std::time::Duration,
) -> Result<(), PciError> {
    let initial_heartbeat = read_heartbeat(chip)?;

    let start = std::time::Instant::now();
    let mut tick_time = std::time::Instant::now();
    let mut tick_count = 0;
    loop {
        let heartbeat = read_heartbeat(chip)?;

        if heartbeat
            .iter()
            .zip(initial_heartbeat.iter())
            .all(|(a, b)| a != b)
        {
            break;
        }

        if tick_count == 0 {
            eprintln!("[{tick_count}; {coord}] Waiting for ethernet ports to train...");
            tick_count += 1;
        }

        if start.elapsed() > timeout {
            panic!("TIMEOUT")
        }

        if tick_time.elapsed() > std::time::Duration::from_secs(10) {
            tick_time = std::time::Instant::now();
            eprintln!(
                "[{tick_count}; {coord}] Still Waiting... ({}/{})",
                start.elapsed().as_secs(),
                timeout.as_secs()
            );
            tick_count += 1;
        }

        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    Ok(())
}

pub struct RemoteWormholeChip<'a> {
    command_q_addr: u64,

    pub eth_x: u8,
    pub eth_y: u8,

    fake_it: bool,

    pub coord: EthCoord,

    pub chip: &'a mut Wormhole,
}

impl RemoteWormholeChip<'_> {
    pub fn create(
        chip: &mut Wormhole,
        eth_x: u8,
        eth_y: u8,
        fake_it: bool,
        timeout: std::time::Duration,
        coord: EthCoord,
    ) -> Result<RemoteWormholeChip, PciError> {
        wait_for_training_to_complete(chip, coord, timeout)?;

        let command_q_addr = chip.noc(false).read32(eth_x, eth_y, 0x170)?;

        Ok(RemoteWormholeChip {
            command_q_addr: command_q_addr as u64,
            eth_x,
            eth_y,
            fake_it,
            chip,
            coord,
        })
    }

    pub fn read32(&mut self, x: u8, y: u8, noc_id: bool, addr: u64) -> Result<u32, PciError> {
        read32(
            self.chip,
            self.command_q_addr as u32,
            EthCommCoord {
                coord: self.coord,
                noc_id: noc_id as u8,
                noc_x: x,
                noc_y: y,
                offset: addr,
            },
            self.eth_x,
            self.eth_y,
            std::time::Duration::from_secs(1),
        )
    }

    pub fn block_read(
        &mut self,
        x: u8,
        y: u8,
        noc_id: bool,
        addr: u64,
        data: &mut [u8],
    ) -> Result<(), PciError> {
        block_read(
            self.chip,
            self.command_q_addr as u32,
            self.eth_x,
            self.eth_y,
            std::time::Duration::from_secs(1),
            self.fake_it,
            EthCommCoord {
                coord: self.coord,
                noc_id: noc_id as u8,
                noc_x: x,
                noc_y: y,
                offset: addr,
            },
            data,
        )
    }

    pub fn write32(
        &mut self,
        x: u8,
        y: u8,
        noc_id: bool,
        addr: u64,
        data: u32,
    ) -> Result<(), PciError> {
        write32(
            self.chip,
            self.command_q_addr as u32,
            EthCommCoord {
                coord: self.coord,
                noc_id: noc_id as u8,
                noc_x: x,
                noc_y: y,
                offset: addr,
            },
            self.eth_x,
            self.eth_y,
            std::time::Duration::from_secs(1),
            data,
        )
    }

    pub fn block_write(
        &mut self,
        x: u8,
        y: u8,
        noc_id: bool,
        addr: u64,
        data: &[u8],
    ) -> Result<(), PciError> {
        block_write(
            self.chip,
            self.command_q_addr as u32,
            self.eth_x,
            self.eth_y,
            std::time::Duration::from_secs(1),
            self.fake_it,
            EthCommCoord {
                coord: self.coord,
                noc_id: noc_id as u8,
                noc_x: x,
                noc_y: y,
                offset: addr,
            },
            data,
        )
    }

    pub fn board_id(&mut self) -> Result<u64, PciError> {
        Ok(
            ((self.read32(0, 10, false, 0x8_1000_0000 + 0x78828 + 0x10C)? as u64) << 32)
                | self.read32(0, 10, false, 0x8_1000_0000 + 0x78828 + 0x108)? as u64,
        )
    }
}
