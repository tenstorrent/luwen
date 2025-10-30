use bytemuck::{Pod, Zeroable};
use std::fmt;
use std::mem;

// define constants for boot fs
const IMAGE_TAG_SIZE: u32 = 8;

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
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct TtBootFsFd {
    pub spi_addr: u32,
    pub copy_dest: u32,
    pub flags: FdFlags,
    pub data_crc: u32,
    pub security_flags: SecurityFdFlags,
    pub image_tag: [u8; IMAGE_TAG_SIZE as usize],
    pub fd_crc: u32,
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

impl PartialEq for TtBootFsFd {
    fn eq(&self, other: &Self) -> bool {
        self.spi_addr == other.spi_addr
            && self.copy_dest == other.copy_dest
            && self.flags == other.flags
            && self.data_crc == other.data_crc
            && self.security_flags == other.security_flags
            && self.image_tag == other.image_tag
            && self.fd_crc == other.fd_crc
    }
}

impl fmt::Debug for TtBootFsFd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TtBootFsFd {{ spi_addr: {}, copy_dest: {}, flags: {:?}, data_crc: {}, security_flags: {:?}, image_tag: {:?}, fd_crc: {} }}",
               self.spi_addr, self.copy_dest, self.flags, self.data_crc, self.security_flags, self.image_tag, self.fd_crc)
    }
}

pub fn read_fd(reader: impl Fn(u32, usize) -> Vec<u8>, addr: u32) -> Option<TtBootFsFd> {
    let fd_bytes = reader(addr, mem::size_of::<TtBootFsFd>());
    if fd_bytes.len() == mem::size_of::<TtBootFsFd>() {
        Some(unsafe {
            mem::transmute::<[u8; mem::size_of::<TtBootFsFd>()], TtBootFsFd>(
                fd_bytes.try_into().unwrap(),
            )
        })
    } else {
        None
    }
}

pub fn read_tag(reader: impl Fn(u32, usize) -> Vec<u8>, tag: &str) -> Option<(u32, TtBootFsFd)> {
    let mut curr_addr = 0;
    loop {
        let fd = read_fd(&reader, curr_addr).unwrap();
        if fd.flags.invalid() {
            return None;
        }
        if fd.image_tag_str() == tag {
            return Some((curr_addr, fd));
        }
        curr_addr += mem::size_of::<TtBootFsFd>() as u32;
    }
}
