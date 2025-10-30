// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::chip_interface::ChipInterface;

#[derive(Error, Debug)]
pub enum AxiError {
    #[error("Invalid path: {key} specifically was not able to find {path}")]
    InvalidPath { key: String, path: String },

    #[error("Invalid path: {key} specifically was not able to parse {path} as an array def.")]
    InvalidArrayPath { key: String, path: String },

    #[error("No AXI data table loaded")]
    NoAxiData,

    #[error("The readbuffer is not large enough to hold the requested data")]
    ReadBufferTooSmall,

    #[error("The writebuffer is not the same size as the requested field")]
    WriteBufferMismatch,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AxiData {
    pub addr: u64,
    pub size: u64,
    pub bits: Option<(u32, u32)>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct MemorySlice {
    pub name: String,
    pub offset: u64,
    pub size: u64,
    pub array_count: Option<u64>,
    pub bit_mask: Option<(u32, u32)>,
    pub children: std::collections::HashMap<String, MemorySlice>,
}

#[derive(Serialize, Deserialize)]
pub enum MemorySlices {
    Flat(std::collections::HashMap<String, AxiData>),
    Tree(std::collections::HashMap<String, MemorySlice>),
}

#[derive(RustEmbed)]
#[folder = "../../axi-data"]
struct WHPciData;

pub fn load_axi_table(file: &str, _version: u32) -> MemorySlices {
    let data = WHPciData::get(file).unwrap();
    bincode::deserialize(&data.data).unwrap()
}

/// This is a generic trait which defines the high level chip communication primatives.
/// It's functions allow for the reading and writing of data to arbirary noc endpoints on any chip
/// with the details of how the endpoint is accessed abstracted away.
///
/// For the ARC endpoint special functions are defined because unlike most noc endpoints the ARC addresses
/// are mapped into the pci BAR address space.
pub trait ChipComms {
    /// Translate a String path into the corresponding AXI address.
    fn axi_translate(&self, addr: &str) -> Result<AxiData, AxiError>;
    /// Read and write to the NOC using AXI address gotten from `axi_translate`.
    fn axi_read(
        &self,
        chip_if: &dyn ChipInterface,
        addr: u64,
        data: &mut [u8],
    ) -> Result<(), Box<dyn std::error::Error>>;
    fn axi_write(
        &self,
        chip_if: &dyn ChipInterface,
        addr: u64,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>>;

    /// Read and write to a noc endpoint, this could be a local or remote chip.
    fn noc_read(
        &self,
        chip_if: &dyn ChipInterface,
        noc_id: u8,
        x: u8,
        y: u8,
        addr: u64,
        data: &mut [u8],
    ) -> Result<(), Box<dyn std::error::Error>>;
    fn noc_write(
        &self,
        chip_if: &dyn ChipInterface,
        noc_id: u8,
        x: u8,
        y: u8,
        addr: u64,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>>;
    fn noc_multicast(
        &self,
        chip_if: &dyn ChipInterface,
        noc_id: u8,
        start: (u8, u8),
        end: (u8, u8),
        addr: u64,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>>;
    fn noc_broadcast(
        &self,
        chip_if: &dyn ChipInterface,
        noc_id: u8,
        addr: u64,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>>;

    /// Convenience functions for reading and writing 32 bit values.
    fn noc_read32(
        &self,
        chip_if: &dyn ChipInterface,
        noc_id: u8,
        x: u8,
        y: u8,
        addr: u64,
    ) -> Result<u32, Box<dyn std::error::Error>> {
        let mut value = [0; 4];
        self.noc_read(chip_if, noc_id, x, y, addr, &mut value)?;
        Ok(u32::from_le_bytes(value))
    }

    fn noc_write32(
        &self,
        chip_if: &dyn ChipInterface,
        noc_id: u8,
        x: u8,
        y: u8,
        addr: u64,
        value: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.noc_write(chip_if, noc_id, x, y, addr, value.to_le_bytes().as_slice())
    }

    fn noc_multicast32(
        &self,
        chip_if: &dyn ChipInterface,
        noc_id: u8,
        start: (u8, u8),
        end: (u8, u8),
        addr: u64,
        value: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.noc_multicast(
            chip_if,
            noc_id,
            start,
            end,
            addr,
            value.to_le_bytes().as_slice(),
        )
    }

    fn noc_broadcast32(
        &self,
        chip_if: &dyn ChipInterface,
        noc_id: u8,
        addr: u64,
        value: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.noc_broadcast(chip_if, noc_id, addr, value.to_le_bytes().as_slice())
    }

    fn axi_read32(
        &self,
        chip_if: &dyn ChipInterface,
        addr: u64,
    ) -> Result<u32, Box<dyn std::error::Error>> {
        let mut value = [0; 4];
        self.axi_read(chip_if, addr, &mut value)?;
        Ok(u32::from_le_bytes(value))
    }

    fn axi_write32(
        &self,
        chip_if: &dyn ChipInterface,
        addr: u64,
        value: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.axi_write(chip_if, addr, value.to_le_bytes().as_slice())
    }

    fn axi_sread32(
        &self,
        chip_if: &dyn ChipInterface,
        addr: &str,
    ) -> Result<u32, Box<dyn std::error::Error>> {
        let addr = self.axi_translate(addr.as_ref())?.addr;

        let mut value = [0; 4];
        self.axi_read(chip_if, addr, &mut value)?;
        Ok(u32::from_le_bytes(value))
    }

    fn axi_swrite32(
        &self,
        chip_if: &dyn ChipInterface,
        addr: &str,
        value: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let addr = self.axi_translate(addr.as_ref())?.addr;

        self.axi_write(chip_if, addr, &value.to_le_bytes())?;
        Ok(())
    }
}

pub fn axi_translate_tree(
    data: &std::collections::HashMap<String, MemorySlice>,
    addr: &str,
) -> Result<AxiData, AxiError> {
    let mut it = data;

    let mut offset = 0;
    let mut size = 0;
    let mut bits = None;
    for key in addr.split('.') {
        let (slice, index) = if !it.contains_key(key) && key.contains('[') {
            let mut parts = key.split('[');
            let key = parts.next().ok_or_else(|| AxiError::InvalidArrayPath {
                key: key.to_string(),
                path: addr.to_string(),
            })?;
            let index = parts
                .next()
                .ok_or_else(|| AxiError::InvalidArrayPath {
                    key: key.to_string(),
                    path: addr.to_string(),
                })?
                .trim_end_matches(']')
                .parse::<u64>()
                .map_err(|_| AxiError::InvalidArrayPath {
                    key: key.to_string(),
                    path: addr.to_string(),
                })?;

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
            bits = slice.bit_mask;
            offset += slice.offset + slice.size * index.unwrap_or(0);
        } else {
            return Err(AxiError::InvalidPath {
                path: addr.to_string(),
                key: key.to_string(),
            });
        }
    }

    Ok(AxiData {
        addr: offset,
        size,
        bits,
    })
}

pub fn axi_translate(data: Option<&MemorySlices>, addr: &str) -> Result<AxiData, AxiError> {
    match data.ok_or(AxiError::NoAxiData)? {
        MemorySlices::Flat(data) => {
            if let Some(data) = data.get(addr) {
                Ok(data.clone())
            } else {
                Err(AxiError::InvalidPath {
                    path: addr.to_string(),
                    key: addr.to_string(),
                })
            }
        }
        MemorySlices::Tree(data) => axi_translate_tree(data, addr),
    }
}

pub struct ArcIf {
    pub axi_data: MemorySlices,
}

impl ChipComms for ArcIf {
    fn axi_translate(&self, addr: &str) -> Result<AxiData, AxiError> {
        axi_translate(Some(&self.axi_data), addr)
    }

    fn axi_read(
        &self,
        chip_if: &dyn ChipInterface,
        addr: u64,
        data: &mut [u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        chip_if.axi_read(addr as u32, data)
    }

    fn axi_write(
        &self,
        chip_if: &dyn ChipInterface,
        addr: u64,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        chip_if.axi_write(addr as u32, data)
    }

    fn noc_read(
        &self,
        chip_if: &dyn ChipInterface,
        noc_id: u8,
        x: u8,
        y: u8,
        addr: u64,
        data: &mut [u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        chip_if.noc_read(noc_id, x, y, addr, data)
    }

    fn noc_write(
        &self,
        chip_if: &dyn ChipInterface,
        noc_id: u8,
        x: u8,
        y: u8,
        addr: u64,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        chip_if.noc_write(noc_id, x, y, addr, data)
    }

    fn noc_multicast(
        &self,
        chip_if: &dyn ChipInterface,
        noc_id: u8,
        start: (u8, u8),
        end: (u8, u8),
        addr: u64,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        chip_if.noc_multicast(noc_id, start, end, addr, data)
    }

    fn noc_broadcast(
        &self,
        chip_if: &dyn ChipInterface,
        noc_id: u8,
        addr: u64,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        chip_if.noc_broadcast(noc_id, addr, data)
    }
}

pub struct NocIf {
    pub axi_data: MemorySlices,
    pub noc_id: u8,
    pub x: u8,
    pub y: u8,
}

impl ChipComms for NocIf {
    fn axi_translate(&self, addr: &str) -> Result<AxiData, AxiError> {
        axi_translate(Some(&self.axi_data), addr)
    }

    fn axi_read(
        &self,
        chip_if: &dyn ChipInterface,
        addr: u64,
        data: &mut [u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        chip_if.noc_read(self.noc_id, self.x, self.y, addr, data)
    }

    fn axi_write(
        &self,
        chip_if: &dyn ChipInterface,
        addr: u64,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        chip_if.noc_write(self.noc_id, self.x, self.y, addr, data)
    }

    fn noc_read(
        &self,
        chip_if: &dyn ChipInterface,
        noc_id: u8,
        x: u8,
        y: u8,
        addr: u64,
        data: &mut [u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        chip_if.noc_read(noc_id, x, y, addr, data)
    }

    fn noc_write(
        &self,
        chip_if: &dyn ChipInterface,
        noc_id: u8,
        x: u8,
        y: u8,
        addr: u64,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        chip_if.noc_write(noc_id, x, y, addr, data)
    }

    fn noc_multicast(
        &self,
        chip_if: &dyn ChipInterface,
        noc_id: u8,
        start: (u8, u8),
        end: (u8, u8),
        addr: u64,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        chip_if.noc_multicast(noc_id, start, end, addr, data)
    }

    fn noc_broadcast(
        &self,
        chip_if: &dyn ChipInterface,
        noc_id: u8,
        addr: u64,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        chip_if.noc_broadcast(noc_id, addr, data)
    }
}

impl ChipComms for Arc<dyn ChipComms> {
    fn axi_translate(&self, addr: &str) -> Result<AxiData, AxiError> {
        self.as_ref().axi_translate(addr)
    }

    fn axi_read(
        &self,
        chip_if: &dyn ChipInterface,
        addr: u64,
        data: &mut [u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.as_ref().axi_read(chip_if, addr, data)
    }

    fn axi_write(
        &self,
        chip_if: &dyn ChipInterface,
        addr: u64,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.as_ref().axi_write(chip_if, addr, data)
    }

    fn noc_read(
        &self,
        chip_if: &dyn ChipInterface,
        noc_id: u8,
        x: u8,
        y: u8,
        addr: u64,
        data: &mut [u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.as_ref().noc_read(chip_if, noc_id, x, y, addr, data)
    }

    fn noc_write(
        &self,
        chip_if: &dyn ChipInterface,
        noc_id: u8,
        x: u8,
        y: u8,
        addr: u64,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.as_ref().noc_write(chip_if, noc_id, x, y, addr, data)
    }

    fn noc_multicast(
        &self,
        chip_if: &dyn ChipInterface,
        noc_id: u8,
        start: (u8, u8),
        end: (u8, u8),
        addr: u64,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.as_ref()
            .noc_multicast(chip_if, noc_id, start, end, addr, data)
    }

    fn noc_broadcast(
        &self,
        chip_if: &dyn ChipInterface,
        noc_id: u8,
        addr: u64,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.as_ref().noc_broadcast(chip_if, noc_id, addr, data)
    }
}

impl ChipComms for Arc<dyn ChipComms + Send + Sync> {
    fn axi_translate(&self, addr: &str) -> Result<AxiData, AxiError> {
        self.as_ref().axi_translate(addr)
    }

    fn axi_read(
        &self,
        chip_if: &dyn ChipInterface,
        addr: u64,
        data: &mut [u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.as_ref().axi_read(chip_if, addr, data)
    }

    fn axi_write(
        &self,
        chip_if: &dyn ChipInterface,
        addr: u64,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.as_ref().axi_write(chip_if, addr, data)
    }

    fn noc_read(
        &self,
        chip_if: &dyn ChipInterface,
        noc_id: u8,
        x: u8,
        y: u8,
        addr: u64,
        data: &mut [u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.as_ref().noc_read(chip_if, noc_id, x, y, addr, data)
    }

    fn noc_write(
        &self,
        chip_if: &dyn ChipInterface,
        noc_id: u8,
        x: u8,
        y: u8,
        addr: u64,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.as_ref().noc_write(chip_if, noc_id, x, y, addr, data)
    }

    fn noc_multicast(
        &self,
        chip_if: &dyn ChipInterface,
        noc_id: u8,
        start: (u8, u8),
        end: (u8, u8),
        addr: u64,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.as_ref()
            .noc_multicast(chip_if, noc_id, start, end, addr, data)
    }

    fn noc_broadcast(
        &self,
        chip_if: &dyn ChipInterface,
        noc_id: u8,
        addr: u64,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.as_ref().noc_broadcast(chip_if, noc_id, addr, data)
    }
}
