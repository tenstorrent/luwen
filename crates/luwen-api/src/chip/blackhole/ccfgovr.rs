// SPDX-FileCopyrightText: © 2026 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

//! `cmfwcfg` override banks (`ccfgovra`, `ccfgovrb`).
//!
//! Two 4 KiB SPI partitions hold a 20-byte header followed by a sparse
//! `FwTableOverride` protobuf body. At boot the FW decodes the newer-seq
//! valid bank's body into a fresh `FwTableOverride` (PB_DECODE_NULLTERMINATED)
//! and applies each present field on top of the cmfwcfg-loaded `FwTable`.
//!
//! Writes always go to the inactive bank, leaving the active one as a
//! torn-write fallback.
//!
//! Header `cksum` is an IEEE CRC32 over the first 16 bytes of the header
//! (everything before `cksum` itself) followed by the body bytes.

use bytemuck::{bytes_of, from_bytes, Pod, Zeroable};
use prost::Message;
use serde_json::Value;
use std::collections::HashMap;
use tracing::{debug, info, trace, warn};

use super::spirom_tables::{self, fw_table_override::FwTableOverride};
use super::Blackhole;

pub const TAG_A: &str = "ccfgovra";
pub const TAG_B: &str = "ccfgovrb";

pub const MAGIC: u32 = 0x564F_4343;
pub const SEQ_ERASED: u32 = 0xFFFF_FFFF;
/// Header framing version. Bumped on any change to `BankHeader` layout.
pub const HDR_VERSION: u32 = 0;
pub const HEADER_LEN: u32 = 20;
pub const BODY_MAX: u32 = 512;
#[allow(dead_code)]
pub const BANK_LEN: u32 = 4096;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct BankHeader {
    pub magic: u32,
    pub seq: u32,
    pub body_len: u32,
    pub version: u32,
    /// IEEE CRC32 of `magic || seq || body_len || version || body` (LE).
    pub cksum: u32,
}

/// Number of header bytes covered by the CRC (everything before `cksum`).
const CKSUM_HDR_LEN: usize = 16;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Bank {
    A,
    B,
}

impl std::fmt::Display for Bank {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Bank::A => write!(f, "A"),
            Bank::B => write!(f, "B"),
        }
    }
}

/// One bank's read-back state.
pub struct BankRead {
    pub bank: Bank,
    pub spi_addr: u32,
    /// `None` if the header failed plausibility checks.
    pub header: Option<BankHeader>,
    /// `None` if the body failed cksum or pb_decode. `Some({})` for a valid
    /// empty body (`body_len == 0`).
    pub body: Option<HashMap<String, Value>>,
}

/// Both banks' read-back state.
pub struct State {
    pub a: BankRead,
    pub b: BankRead,
}

impl State {
    pub fn read(bh: &Blackhole) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(State {
            a: read_bank(bh, Bank::A)?,
            b: read_bank(bh, Bank::B)?,
        })
    }

    /// The active bank — the one the firmware would use at boot.
    /// `None` if neither bank validates.
    pub fn active(&self) -> Option<&BankRead> {
        let picked = match (self.a.body.is_some(), self.b.body.is_some()) {
            (false, false) => None,
            (true, false) => Some(&self.a),
            (false, true) => Some(&self.b),
            (true, true) => {
                let a_seq = self.a.header.expect("body Some implies header Some").seq;
                let b_seq = self.b.header.expect("body Some implies header Some").seq;
                if seq_is_newer(b_seq, a_seq) {
                    Some(&self.b)
                } else {
                    Some(&self.a)
                }
            }
        };
        if let Some(b) = picked {
            debug!(
                bank = %b.bank,
                seq = b.header.expect("header present").seq,
                "ccfgovr active bank",
            );
        } else {
            debug!("ccfgovr has no valid bank; cmfwcfg used as-is");
        }
        picked
    }

    /// The bank to write to next — the opposite of `active()`. If neither
    /// bank validates, defaults to A (so the first write lands in A and the
    /// FW picks it up next boot).
    pub fn inactive(&self) -> &BankRead {
        match self.active() {
            Some(BankRead { bank: Bank::A, .. }) => &self.b,
            Some(BankRead { bank: Bank::B, .. }) => &self.a,
            _ => &self.a,
        }
    }

    /// The active bank's override map, or an empty map if no bank is valid.
    pub fn override_map(&self) -> HashMap<String, Value> {
        self.active()
            .and_then(|b| b.body.clone())
            .unwrap_or_default()
    }
}

/// Read and validate both ccfgovr banks; return the active bank's body, or
/// an empty map if neither is valid.
pub fn read_active(bh: &Blackhole) -> Result<HashMap<String, Value>, Box<dyn std::error::Error>> {
    Ok(State::read(bh)?.override_map())
}

/// Merge the active override on top of cmfwcfg and return the effective
/// fw_table view (what the firmware would actually load at boot).
pub fn read_effective(
    bh: &Blackhole,
) -> Result<HashMap<String, Value>, Box<dyn std::error::Error>> {
    let mut effective = bh.decode_boot_fs_table("cmfwcfg")?;
    for (k, v) in State::read(bh)?.override_map() {
        effective.insert(k, v);
    }
    Ok(effective)
}

/// Write `new_map` as the next override to the inactive bank. Returns the
/// bank that is now active (the one we just wrote).
pub fn write(
    bh: &Blackhole,
    new_map: HashMap<String, Value>,
) -> Result<Bank, Box<dyn std::error::Error>> {
    let state = State::read(bh)?;
    let active_seq = state.active().and_then(|a| a.header).map(|h| h.seq);
    let inactive = state.inactive();

    // Encode the override body. The FW decodes with PB_DECODE_NULLTERMINATED,
    // so a non-empty body needs a 0-byte terminator. body_len == 0 is a
    // special case the FW treats as "empty override, no decode" — match
    // that to keep our writes byte-identical to the initial empty bank.
    let proto_bytes = spirom_tables::from_hash_map::<FwTableOverride>(new_map).encode_to_vec();
    let body = if proto_bytes.is_empty() {
        Vec::new()
    } else {
        let mut b = proto_bytes;
        b.push(0);
        while b.len() % 4 != 0 {
            b.push(0);
        }
        b
    };
    if body.len() as u32 > BODY_MAX {
        return Err(format!(
            "ccfgovr body size {} exceeds maximum {}",
            body.len(),
            BODY_MAX
        )
        .into());
    }

    let new_seq = match active_seq {
        Some(s) if s != SEQ_ERASED => s.wrapping_add(1),
        _ => 0,
    };
    let body_len = body.len() as u32;
    let mut hdr = BankHeader {
        magic: MAGIC,
        seq: new_seq,
        body_len,
        version: HDR_VERSION,
        cksum: 0,
    };
    hdr.cksum = compute_cksum(&hdr, &body);

    let mut payload = Vec::with_capacity(HEADER_LEN as usize + body.len());
    payload.extend_from_slice(bytes_of(&hdr));
    payload.extend_from_slice(&body);

    info!(
        bank = %inactive.bank,
        seq = new_seq,
        body_len,
        cksum = format_args!("0x{:08x}", hdr.cksum),
        "ccfgovr write",
    );
    bh.spi_write(inactive.spi_addr, &payload)?;

    let mut readback = vec![0u8; payload.len()];
    bh.spi_read(inactive.spi_addr, &mut readback)?;
    if readback != payload {
        return Err("ccfgovr write verification failed: readback differs from payload".into());
    }
    trace!(bank = %inactive.bank, "ccfgovr write verified");

    Ok(inactive.bank)
}

fn read_bank(bh: &Blackhole, bank: Bank) -> Result<BankRead, Box<dyn std::error::Error>> {
    let tag = match bank {
        Bank::A => TAG_A,
        Bank::B => TAG_B,
    };
    let (_, fd) = bh
        .get_boot_fs_tables_spi_read(tag)?
        .ok_or_else(|| format!("ccfgovr partition '{tag}' not found in boot FS"))?;
    let spi_addr = fd.spi_addr;

    let mut hdr_bytes = [0u8; HEADER_LEN as usize];
    bh.spi_read(spi_addr, &mut hdr_bytes)?;
    let hdr: BankHeader = *from_bytes(&hdr_bytes);
    trace!(
        bank = %bank,
        spi_addr = format_args!("0x{spi_addr:08x}"),
        magic = format_args!("0x{:08x}", hdr.magic),
        seq = hdr.seq,
        body_len = hdr.body_len,
        version = hdr.version,
        cksum = format_args!("0x{:08x}", hdr.cksum),
        "ccfgovr bank header",
    );

    if !is_header_plausible(&hdr) {
        debug!(bank = %bank, "ccfgovr header failed plausibility check");
        return Ok(BankRead {
            bank,
            spi_addr,
            header: None,
            body: None,
        });
    }

    let mut body_bytes = vec![0u8; hdr.body_len as usize];
    if !body_bytes.is_empty() {
        bh.spi_read(spi_addr + HEADER_LEN, &mut body_bytes)?;
    }
    let computed = compute_cksum(&hdr, &body_bytes);
    if computed != hdr.cksum {
        warn!(
            bank = %bank,
            expected = format_args!("0x{:08x}", hdr.cksum),
            computed = format_args!("0x{computed:08x}"),
            "ccfgovr CRC32 mismatch",
        );
        return Ok(BankRead {
            bank,
            spi_addr,
            header: Some(hdr),
            body: None,
        });
    }

    let map = if hdr.body_len == 0 {
        HashMap::new()
    } else {
        // Strip the null terminator and trailing zero pad to recover the
        // protobuf prefix. Encoded proto3 messages never end in 0x00 (a
        // 0-byte at this position is either the appended null terminator
        // or padding), so scanning backward for the last nonzero byte is
        // unambiguous.
        let proto_end = body_bytes
            .iter()
            .rposition(|&b| b != 0)
            .map_or(0, |p| p + 1);
        spirom_tables::to_hash_map(FwTableOverride::decode(&body_bytes[..proto_end])?)
    };

    Ok(BankRead {
        bank,
        spi_addr,
        header: Some(hdr),
        body: Some(map),
    })
}

fn is_header_plausible(hdr: &BankHeader) -> bool {
    hdr.magic == MAGIC
        && hdr.seq != SEQ_ERASED
        && hdr.version == HDR_VERSION
        && hdr.body_len % 4 == 0
        && hdr.body_len <= BODY_MAX
}

fn seq_is_newer(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) > 0
}

fn compute_cksum(hdr: &BankHeader, body: &[u8]) -> u32 {
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(&bytes_of(hdr)[..CKSUM_HDR_LEN]);
    hasher.update(body);
    hasher.finalize()
}
