use std::collections::HashMap;

use kmdif::{PciDevice, PciError};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Deserialize, Serialize)]
pub struct MemorySlice {
    pub name: String,
    pub offset: u64,
    pub size: u64,
    pub array_count: Option<u64>,
    pub bit_mask: Option<(u64, u64)>,
    pub children: HashMap<String, MemorySlice>,
}

pub struct Axi {
    slices: HashMap<String, MemorySlice>,
}

pub struct AxiReadWrite<'a> {
    pub axi: &'a Axi,
    pub transport: &'a mut PciDevice,
}

#[derive(Error, Debug)]
pub enum AxiError {
    #[error("Invalid path: {0} specifically was not found {1}")]
    InvalidPath(String, String),

    #[error(transparent)]
    PciError(#[from] PciError),
}

impl Axi {
    pub fn new(file: &str) -> Axi {
        let file = std::fs::read(file).unwrap();
        Axi {
            slices: bincode::deserialize(&file).unwrap(),
        }
    }

    pub fn empty() -> Axi {
        Axi {
            slices: HashMap::new(),
        }
    }

    fn lookup(&self, path: &str) -> Result<(u64, u64, Option<(u64, u64)>), AxiError> {
        let mut it = &self.slices;

        let mut offset = 0;
        let mut size = 0;
        let mut bits = None;
        for key in path.split('.') {
            let (slice, index) = if !it.contains_key(key) && key.contains('[') {
                let mut parts = key.split('[');
                let key = parts.next().unwrap();
                let index = parts
                    .next()
                    .unwrap()
                    .trim_end_matches(']')
                    .parse::<u64>()
                    .unwrap();

                (it.get(key), Some(index))
            } else {
                (it.get(key), None)
            };

            if let Some(slice) = slice {
                it = &slice.children;
                if let (Some(count), Some(index)) = (slice.array_count, index) {
                    assert!(index < count);
                } else if index.is_some() ^ slice.array_count.is_some() {
                    dbg!(slice);
                    panic!("Tried to index a non-array or no index for array with {key}")
                }

                size = slice.size;
                bits = slice.bit_mask.clone();
                offset += slice.offset + slice.size * index.unwrap_or(0);
            } else {
                return Err(AxiError::InvalidPath(path.to_string(), key.to_string()));
            }
        }

        Ok((offset, size, bits))
    }
}

impl AxiReadWrite<'_> {
    pub fn read<T>(&mut self, path: &str) -> Result<T, AxiError> {
        let (offset, size, bit_mask) = self.axi.lookup(path)?;

        let data = if size > 4 {
            let mut data = vec![0u8; size as usize];
            self.transport.read_block(offset as u32, &mut data)?;

            data
        } else {
            let data = self.transport.read32(offset as u32)?;

            data.to_ne_bytes().to_vec()
        };

        let mut data = if let Some((lsb, msb)) = bit_mask {
            let bytes_to_skip = lsb / 8;
            let bits_to_shift = (lsb % 8) as u8;

            let to_end_on = msb / 8;
            let bits_to_keep = (msb % 8) as u8;

            let mut shifted_data = vec![];
            for byte in data[bytes_to_skip as usize..(to_end_on as usize + 1)].iter() {
                if let Some(data) = shifted_data.last_mut() {
                    *data |= byte & ((1 << bits_to_shift as u32) - 1);
                } else {
                    shifted_data.push(*byte >> bits_to_shift);
                }
            }

            if let Some(data) = shifted_data.last_mut() {
                *data &= (1 << bits_to_keep as u32) - 1;
            }

            shifted_data
        } else {
            data
        };

        data.shrink_to(0);

        assert_eq!(std::mem::size_of::<T>(), data.len());

        let data = data.leak();

        // Lifted from std::mem::transmute_copy
        let output = if std::mem::align_of::<T>() > std::mem::align_of::<&[u8]>() {
            unsafe { std::ptr::read_unaligned(data.as_ptr() as *const T) }
        } else {
            unsafe { std::ptr::read(data.as_ptr() as *const T) }
        };

        Ok(output)
    }

    pub fn write(&mut self, path: &str, value: &[u8]) -> Result<(), AxiError> {
        let (offset, size, bit_mask) = self.axi.lookup(path)?;

        assert!(bit_mask.is_none());

        assert_eq!(size as usize, value.len());

        if size > 4 {
            self.transport.write_block(offset as u32, value)?;
        } else {
            self.transport
                .write32(offset as u32, u32::from_ne_bytes(value.try_into().unwrap()))?;
        }

        Ok(())
    }
}
