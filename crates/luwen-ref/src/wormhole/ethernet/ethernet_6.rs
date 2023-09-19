use kmdif::PciError;
use luwen_if::EthAddr;

#[derive(Clone, Debug)]
pub struct EthCommCoord {
    pub coord: EthAddr,
    pub noc_id: u8,
    pub noc_x: u8,
    pub noc_y: u8,
    pub offset: u64,
}

pub fn get_rack_addr(coord: &EthCommCoord) -> u16 {
    ((coord.coord.rack_y as u16) << 8) | (coord.coord.rack_x as u16)
}

pub fn get_sys_addr(coord: &EthCommCoord) -> u64 {
    let mut addr = coord.coord.shelf_y as u64;
    addr = (addr << 6) | (coord.coord.shelf_x as u64);
    addr = (addr << 6) | (coord.noc_y as u64);
    addr = (addr << 6) | (coord.noc_x as u64);
    addr = (addr << 36) | (coord.offset as u64);

    addr
}

const Q_NAME: [&'static str; 4] = [
    "REQ CMD Q",
    "ETH IN REQ CMD Q",
    "RESP CMD Q",
    "ETH OUT REQ CMD Q",
];

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
    read32: &mut impl FnMut(u32) -> Result<u32, PciError>,
    command_q_addr: u32,
    timeout: std::time::Duration,
) -> Result<u32, PciError> {
    let mut curr_wptr = read32(command_q_addr + REQ_Q_ADDR + 4 * WR_PTR_OFFSET)?;

    let start = std::time::Instant::now();
    loop {
        let curr_rptr = read32(command_q_addr + REQ_Q_ADDR + 4 * RD_PTR_OFFSET)?;

        let is_command_q_full = (curr_wptr != curr_rptr)
            && ((curr_wptr & CMD_BUF_SIZE_MASK) == (curr_rptr & CMD_BUF_SIZE_MASK));

        if !is_command_q_full {
            break;
        }

        if start.elapsed() > timeout {
            panic!("TIMEOUT")
        }
        curr_wptr = read32(command_q_addr + REQ_Q_ADDR + 4 * WR_PTR_OFFSET)?;
    }

    Ok(curr_wptr)
}

pub fn eth_read32(
    read32: &mut impl FnMut(u32) -> Result<u32, PciError>,
    write32: &mut impl FnMut(u32, u32) -> Result<(), PciError>,
    command_q_addr: u32,
    coord: EthCommCoord,
    timeout: std::time::Duration,
) -> Result<u32, PciError> {
    let curr_wptr = wait_for_idle(read32, command_q_addr, timeout)?;

    let cmd_addr =
        command_q_addr + REQ_Q_ADDR + 4 * CMD_OFFSET + (curr_wptr % CMD_BUF_SIZE) * Q_ENTRY_BYTES;

    let sys_addr = get_sys_addr(&coord);
    let rack_addr = get_rack_addr(&coord);

    write32(cmd_addr, (sys_addr & 0xFFFFFFFF) as u32)?;
    write32(cmd_addr + 4, (sys_addr >> 32) as u32)?;
    write32(cmd_addr + 16, rack_addr as u32)?;

    let mut flags = CMD_RD_REQ;
    flags |= (((coord.noc_id as u32) & NOC_ID_MASK) as u32) << NOC_ID_SHIFT;
    write32(cmd_addr + 12, flags)?;

    let next_wptr = (curr_wptr + 1) % (2 * CMD_BUF_SIZE);
    write32(command_q_addr + REQ_Q_ADDR + 4 * WR_PTR_OFFSET, next_wptr)?;

    let curr_rptr = read32(command_q_addr + RESP_Q_ADDR + 4 * RD_PTR_OFFSET)?;
    let mut curr_wptr = read32(command_q_addr + RESP_Q_ADDR + 4 * WR_PTR_OFFSET)?;

    let start_time = std::time::Instant::now();
    while curr_wptr == curr_rptr {
        curr_wptr = read32(command_q_addr + RESP_Q_ADDR + 4 * WR_PTR_OFFSET)?;
        if start_time.elapsed() > timeout {
            panic!("TIMEOUT");
        }
    }

    let cmd_addr =
        command_q_addr + RESP_Q_ADDR + 4 * CMD_OFFSET + (curr_rptr % CMD_BUF_SIZE) * Q_ENTRY_BYTES;

    let mut flags = 0;
    let start_time = std::time::Instant::now();
    while flags == 0 {
        flags = read32(cmd_addr + 12)?;
        if start_time.elapsed() > timeout {
            panic!("TIMEOUT");
        }
    }

    let is_block = (flags & CMD_DATA_BLOCK) == 64;
    let data = read32(cmd_addr + 8)?;

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
    write32(command_q_addr + RESP_Q_ADDR + 4 * RD_PTR_OFFSET, next_rptr)?;

    if let Some(error) = error {
        panic!("{}", error);
    }

    Ok(data)
}

pub fn block_read(
    read32: &mut impl FnMut(u32) -> Result<u32, PciError>,
    write32: &mut impl FnMut(u32, u32) -> Result<(), PciError>,
    get_dma_buffer: &mut impl FnMut() -> Result<kmdif::DmaBuffer, PciError>,
    command_q_addr: u32,
    timeout: std::time::Duration,
    fake_it: bool,
    mut coord: EthCommCoord,
    data: &mut [u8],
) -> Result<(), PciError> {
    if fake_it {
        assert_eq!(data.len() % 4, 0);

        let data = unsafe { std::mem::transmute::<_, &mut [u32]>(data) };
        for d in data {
            *d = eth_read32(read32, write32, command_q_addr, coord.clone(), timeout)?;
            coord.offset += 4;
        }

        return Ok(());
    }

    let rack_addr = get_rack_addr(&coord);

    let buffer = get_dma_buffer()?;
    let mut buffer_pos = 0;

    let number_of_slices = 4;
    let buffer_slice_len = buffer.size / number_of_slices;
    let buffer_size = buffer.size;
    while buffer_pos < buffer_size {
        let sys_addr = get_sys_addr(&coord);

        let curr_wptr = wait_for_idle(read32, command_q_addr, timeout)?;

        let cmd_addr = command_q_addr
            + REQ_Q_ADDR
            + 4 * CMD_OFFSET
            + (curr_wptr % CMD_BUF_SIZE) * Q_ENTRY_BYTES;

        let dma_offset = buffer_slice_len * (curr_wptr as u64 % number_of_slices);

        let buffer = get_dma_buffer()?;

        let dma_phys_pointer = buffer.physical_address + dma_offset;
        let block_len = (data.len() as u64 - buffer_pos).min(buffer_slice_len) as usize;

        write32(cmd_addr, (sys_addr & 0xFFFFFFFF) as u32)?;
        write32(cmd_addr + 4, (sys_addr >> 32) as u32)?;
        write32(cmd_addr + 16, rack_addr as u32)?;

        let mut flags = CMD_RD_REQ;
        flags |= (coord.noc_id as u32 & NOC_ID_MASK) << NOC_ID_SHIFT;
        flags |= CMD_DATA_BLOCK | CMD_DATA_BLOCK_DRAM;
        write32(cmd_addr + 8, block_len as u32)?;
        write32(cmd_addr + 28, dma_phys_pointer as u32)?;
        write32(cmd_addr + 12, flags)?;

        let next_wptr = (curr_wptr + 1) % (2 * CMD_BUF_SIZE);
        write32(command_q_addr + REQ_Q_ADDR + 4 * WR_PTR_OFFSET, next_wptr)?;

        let curr_rptr = read32(command_q_addr + RESP_Q_ADDR + 4 * RD_PTR_OFFSET)?;
        let mut curr_wptr = read32(command_q_addr + RESP_Q_ADDR + 4 * WR_PTR_OFFSET)?;

        let start_time = std::time::Instant::now();
        while curr_wptr == curr_rptr {
            curr_wptr = read32(command_q_addr + RESP_Q_ADDR + 4 * WR_PTR_OFFSET)?;
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
            flags = read32(cmd_addr + 12)?;
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
        write32(command_q_addr + RESP_Q_ADDR + 4 * RD_PTR_OFFSET, next_rptr)?;

        let buffer = get_dma_buffer()?;

        data[buffer_pos as usize..][..block_len]
            .copy_from_slice(&buffer.buffer[dma_offset as usize..][..block_len]);

        buffer_pos += buffer_slice_len;
        coord.offset += buffer_slice_len;
    }

    Ok(())
}

pub fn eth_write32(
    read32: &mut impl FnMut(u32) -> Result<u32, PciError>,
    write32: &mut impl FnMut(u32, u32) -> Result<(), PciError>,
    command_q_addr: u32,
    coord: EthCommCoord,
    timeout: std::time::Duration,
    value: u32,
) -> Result<(), PciError> {
    let curr_wptr = wait_for_idle(read32, command_q_addr, timeout)?;

    let cmd_addr =
        command_q_addr + REQ_Q_ADDR + 4 * CMD_OFFSET + (curr_wptr % CMD_BUF_SIZE) * Q_ENTRY_BYTES;

    let sys_addr = get_sys_addr(&coord);
    let rack_addr = get_rack_addr(&coord);

    write32(cmd_addr, (sys_addr & 0xFFFFFFFF) as u32)?;
    write32(cmd_addr + 4, (sys_addr >> 32) as u32)?;
    write32(cmd_addr + 16, rack_addr as u32)?;

    let mut flags = CMD_WR_REQ;
    flags |= ((coord.noc_id as u32) & NOC_ID_MASK) << NOC_ID_SHIFT;
    write32(cmd_addr + 8, value)?;
    write32(cmd_addr + 12, flags)?;

    let next_wptr = (curr_wptr + 1) % (2 * CMD_BUF_SIZE);
    write32(command_q_addr + REQ_Q_ADDR + 4 * WR_PTR_OFFSET, next_wptr)?;

    Ok(())
}

pub fn block_write(
    read32: &mut impl FnMut(u32) -> Result<u32, PciError>,
    write32: &mut impl FnMut(u32, u32) -> Result<(), PciError>,
    get_dma_buffer: &mut impl FnMut() -> Result<kmdif::DmaBuffer, PciError>,
    command_q_addr: u32,
    timeout: std::time::Duration,
    fake_it: bool,
    mut coord: EthCommCoord,
    data: &[u8],
) -> Result<(), PciError> {
    if fake_it {
        assert_eq!(data.len() % 4, 0);

        let data = unsafe { std::mem::transmute::<_, &[u32]>(data) };
        for d in data {
            eth_write32(read32, write32, command_q_addr, coord.clone(), timeout, *d)?;
            coord.offset += 4;
        }

        return Ok(());
    }

    let rack_addr = get_rack_addr(&coord);

    let buffer = get_dma_buffer()?;
    let mut buffer_pos = 0;

    let number_of_slices = 4;
    let buffer_slice_len = buffer.size / number_of_slices;
    let buffer_size = buffer.size;
    while buffer_pos < buffer_size {
        let sys_addr = get_sys_addr(&coord);

        let curr_wptr = wait_for_idle(read32, command_q_addr, timeout)?;

        let cmd_addr = command_q_addr
            + REQ_Q_ADDR
            + 4 * CMD_OFFSET
            + (curr_wptr % CMD_BUF_SIZE) * Q_ENTRY_BYTES;

        let dma_offset = buffer_slice_len * (curr_wptr as u64 % number_of_slices);

        let mut buffer = get_dma_buffer()?;

        let dma_phys_pointer = buffer.physical_address + dma_offset;
        let block_len = (data.len() as u64 - buffer_pos).min(buffer_slice_len) as usize;

        buffer.buffer[dma_offset as usize..][..block_len]
            .copy_from_slice(&data[buffer_pos as usize..][..block_len]);

        write32(cmd_addr, (sys_addr & 0xFFFFFFFF) as u32)?;
        write32(cmd_addr + 4, (sys_addr >> 32) as u32)?;
        write32(cmd_addr + 16, rack_addr as u32)?;

        let mut flags = CMD_WR_REQ;
        flags |= ((coord.noc_id as u32) & NOC_ID_MASK) << NOC_ID_SHIFT;
        flags |= CMD_DATA_BLOCK | CMD_DATA_BLOCK_DRAM;
        write32(cmd_addr + 8, block_len as u32)?;
        write32(cmd_addr + 28, dma_phys_pointer as u32)?;
        write32(cmd_addr + 12, flags)?;

        let next_wptr = (curr_wptr + 1) % (2 * CMD_BUF_SIZE);
        write32(command_q_addr + REQ_Q_ADDR + 4 * WR_PTR_OFFSET, next_wptr)?;

        buffer_pos += buffer_slice_len;
        coord.offset += buffer_slice_len;
    }

    Ok(())
}

pub fn fixup_queues(
    read32: &mut impl FnMut(u32) -> Result<u32, PciError>,
    write32: &mut impl FnMut(u32, u32) -> Result<(), PciError>,
    command_q_addr: u32,
) -> Result<(), PciError> {
    let i = 2;
    let wr_ptr_addr = command_q_addr + REQ_Q_ADDR + 4 * (i * Q_SIZE_WORDS + WR_PTR_OFFSET);
    let rd_ptr_addr = command_q_addr + REQ_Q_ADDR + 4 * (i * Q_SIZE_WORDS + RD_PTR_OFFSET);
    let wr_ptr = read32(wr_ptr_addr)?;
    let rd_ptr = read32(rd_ptr_addr)?;

    if wr_ptr != rd_ptr {
        dbg!("RESPONSE_Q out of sync - wr_ptr: {wr_ptr}, rd_ptr: {rd_ptr}");
        dbg!("Setting rd_ptr = wr_ptr for the RESP CMD Q");
        write32(rd_ptr_addr, wr_ptr)?;
    }

    Ok(())
}

pub fn print_queue_state(
    read32: &mut impl FnMut(u32) -> Result<u32, PciError>,
    command_q_addr: u32,
    skip_aligned_queues: bool,
) -> Result<(), PciError> {
    let mut rd_addr = command_q_addr + REQ_Q_ADDR;
    let mut q_data = Vec::new();

    for _ in 0..14 {
        let mut j = 0;
        while j < Q_SIZE {
            q_data.push(read32(rd_addr)?);
            j = j + 4;
            rd_addr = rd_addr + 4;
        }
    }

    for i in 0..Q_NAME.len() as u32 {
        println!("{}", Q_NAME[i as usize]);
        let wptr = q_data[(i * Q_SIZE_WORDS + WR_PTR_OFFSET) as usize];
        let rptr = q_data[(i * Q_SIZE_WORDS + RD_PTR_OFFSET) as usize];

        if skip_aligned_queues && wptr == rptr {
            println!("{i} Wptr == Rptr, skipping...");
            continue;
        }

        println!("Wr Ptr = {wptr}");
        println!("Rd Ptr = {rptr}");
        for cmd in 0..4 {
            let cmd_base = i * Q_SIZE_WORDS + CMD_OFFSET + cmd * Q_ENTRY_WORDS;
            println!(
                "Address [{cmd}] = 0x{:08x}{:08x}",
                q_data[(cmd_base + ADDR_H_OFFSET) as usize],
                q_data[(cmd_base + ADDR_L_OFFSET) as usize]
            );
            println!(
                "Data    [{cmd}] = 0x{:08x}",
                q_data[(cmd_base + DATA_OFFSET) as usize]
            );
            println!(
                "Flags   [{cmd}] = 0x{:02x}",
                q_data[(cmd_base + FLAGS_OFFSET) as usize]
            );
            println!(
                "Src Buf [{cmd}] = {}",
                q_data[(cmd_base + SRC_RESP_BUF_INDEX_OFFSET) as usize]
            );
            println!(
                "Lcl Buf [{cmd}] = {}",
                q_data[(cmd_base + LCL_BUF_INDEX_OFFSET) as usize]
            );
            println!(
                "Src QID [{cmd}] = {}",
                q_data[(cmd_base + SRC_RESP_Q_ID_OFFSET) as usize]
            );
            println!(
                "Src Tag [{cmd}] = 0x{:08x}\n",
                q_data[(cmd_base + SRC_ADDR_TAG_OFFSET) as usize]
            );
        }
        println!("==============================");
    }

    Ok(())
}
