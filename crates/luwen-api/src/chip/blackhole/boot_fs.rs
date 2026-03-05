use bytemuck::{Pod, Zeroable};
use std::fmt;
use std::mem;

// define constants for boot fs
const IMAGE_TAG_SIZE: u32 = 8;
const BOOT_FS_HEADER_START: u32 = 0x120000;
const BOOT_FS_HEADER_MAGIC: u32 = 0x54544246; // 'TTBF' in little-endian

#[bitfield_struct::bitfield(u32)] // specify the bitfield size to match the c struct
#[derive(PartialEq, Pod, Zeroable)]
pub struct FdFlags {
    #[bits(24)] // 24 bits for `image_size`
    pub image_size: u32,
    #[bits(1)] // 1 bit for `invalid`
    pub invalid: bool,
    #[bits(1)] // 1 bit for `executable`
    pub executable: bool,
    #[bits(6)] // 6 bits for `fd_flags_rsvd`
    pub fd_flags_rsvd: u8,
}

#[bitfield_struct::bitfield(u32)] // specify the bitfield size to match the c struct
#[derive(PartialEq, Pod, Zeroable)]
pub struct SecurityFdFlags {
    #[bits(12)]
    pub signature_size: u16,
    #[bits(8)]
    pub sb_phase: u8,
    /// Padding
    #[bits(12)]
    __: u16,
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable, PartialEq)]
pub struct TtBootFsFd {
    pub spi_addr: u32,
    pub copy_dest: u32,
    pub flags: FdFlags,
    pub data_crc: u32,
    pub security_flags: SecurityFdFlags,
    pub image_tag: [u8; IMAGE_TAG_SIZE as usize],
    pub fd_crc: u32,
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct TtBootFsHeader {
    pub magic: u32,
    pub version: u32,
    pub num_fds: u32,
}

impl TtBootFsFd {
    pub fn image_tag_str(&self) -> String {
        let nul_pos = self
            .image_tag
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(self.image_tag.len());
        String::from_utf8_lossy(&self.image_tag[..nul_pos]).to_string()
    }
}

impl fmt::Debug for TtBootFsFd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TtBootFsFd {{ spi_addr: {}, copy_dest: {}, flags: {:?}, data_crc: {}, security_flags: {:?}, image_tag: {:?}, fd_crc: {} }}",
               self.spi_addr, self.copy_dest, self.flags, self.data_crc, self.security_flags, self.image_tag, self.fd_crc)
    }
}

fn read_pod<T: Pod>(reader: impl Fn(u32, usize) -> Vec<u8>, addr: u32) -> Option<T> {
    let bytes = reader(addr, mem::size_of::<T>());
    bytes.get(..mem::size_of::<T>()).map(bytemuck::pod_read_unaligned::<T>)
}

pub fn read_fd(reader: impl Fn(u32, usize) -> Vec<u8>, addr: u32) -> Option<TtBootFsFd> {
    read_pod(reader, addr)
}

pub fn read_header(reader: impl Fn(u32, usize) -> Vec<u8>, addr: u32) -> Option<TtBootFsHeader> {
    read_pod(reader, addr)
}

fn find_descriptor_tables(reader: impl Fn(u32, usize) -> Vec<u8>) -> Vec<u32> {
    let mut descriptor_table_addrs = Vec::new();

    if let Some(header) = read_header(&reader, BOOT_FS_HEADER_START) {
        if header.magic == BOOT_FS_HEADER_MAGIC {
            let descriptor_table_list_addr = BOOT_FS_HEADER_START + mem::size_of::<TtBootFsHeader>() as u32;
            let descriptor_table_raw = reader(descriptor_table_list_addr, (header.num_fds * 4) as usize);
            for addr_raw in descriptor_table_raw.chunks_exact(4) {
                let descriptor_table_addr = u32::from_le_bytes(addr_raw.try_into().unwrap());
                descriptor_table_addrs.push(descriptor_table_addr);
            }
        } else {
            // Legacy bootfs. Just has tables at 0x0 and 0x4000
            return vec![0x0, 0x4000];
        }
    }

    descriptor_table_addrs
}

pub fn read_tag(reader: impl Fn(u32, usize) -> Vec<u8>, tag: &str) -> Option<(u32, TtBootFsFd)> {
    let boot_headers = find_descriptor_tables(&reader);

    for mut fd_addr in boot_headers {
        loop {
            let fd = read_fd(&reader, fd_addr).unwrap();
            if fd.flags.invalid() {
                break;
            }
            if fd.image_tag_str() == tag {
                return Some((fd_addr, fd));
            }
            fd_addr += mem::size_of::<TtBootFsFd>() as u32;
        }
    }

    None
}
