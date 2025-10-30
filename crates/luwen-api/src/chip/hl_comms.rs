// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::error::PlatformError;

use super::{AxiData, AxiError, ChipComms, ChipInterface};

/// Convinence trait for high-level communication with an arbitrary chip.
pub trait HlComms {
    fn comms_obj(&self) -> (&dyn ChipComms, &dyn ChipInterface);

    fn noc_read(
        &self,
        noc_id: u8,
        x: u8,
        y: u8,
        addr: u64,
        data: &mut [u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (arc_if, chip_if) = self.comms_obj();
        arc_if.noc_read(chip_if, noc_id, x, y, addr, data)
    }

    fn noc_write(
        &self,
        noc_id: u8,
        x: u8,
        y: u8,
        addr: u64,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (arc_if, chip_if) = self.comms_obj();
        arc_if.noc_write(chip_if, noc_id, x, y, addr, data)
    }

    fn noc_multicast(
        &self,
        noc_id: u8,
        start: (u8, u8),
        end: (u8, u8),
        addr: u64,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (_, chip_if) = self.comms_obj();
        chip_if.noc_multicast(noc_id, start, end, addr, data)
    }

    fn noc_broadcast(
        &self,
        noc_id: u8,
        addr: u64,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (arc_if, chip_if) = self.comms_obj();
        arc_if.noc_broadcast(chip_if, noc_id, addr, data)
    }

    fn noc_read32(
        &self,
        noc_id: u8,
        x: u8,
        y: u8,
        addr: u64,
    ) -> Result<u32, Box<dyn std::error::Error>> {
        let (arc_if, chip_if) = self.comms_obj();
        arc_if.noc_read32(chip_if, noc_id, x, y, addr)
    }

    fn noc_write32(
        &self,
        noc_id: u8,
        x: u8,
        y: u8,
        addr: u64,
        value: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (arc_if, chip_if) = self.comms_obj();
        arc_if.noc_write32(chip_if, noc_id, x, y, addr, value)
    }

    fn noc_broadcast32(
        &self,
        noc_id: u8,
        addr: u64,
        value: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (arc_if, chip_if) = self.comms_obj();
        arc_if.noc_broadcast32(chip_if, noc_id, addr, value)
    }

    fn axi_read(&self, addr: u64, data: &mut [u8]) -> Result<(), Box<dyn std::error::Error>> {
        let (arc_if, chip_if) = self.comms_obj();
        arc_if.axi_read(chip_if, addr, data)
    }

    fn axi_write(&self, addr: u64, data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        let (arc_if, chip_if) = self.comms_obj();
        arc_if.axi_write(chip_if, addr, data)
    }

    fn axi_read32(&self, addr: u64) -> Result<u32, Box<dyn std::error::Error>> {
        let (arc_if, chip_if) = self.comms_obj();
        arc_if.axi_read32(chip_if, addr)
    }

    fn axi_write32(&self, addr: u64, value: u32) -> Result<(), Box<dyn std::error::Error>> {
        let (arc_if, chip_if) = self.comms_obj();
        arc_if.axi_write32(chip_if, addr, value)
    }
}

#[inline]
fn right_shift(existing: &mut [u8], shift: u32) {
    let byte_shift = shift as usize / 8;
    let bit_shift = shift as usize % 8;

    if shift as usize >= existing.len() * 8 {
        for o in existing {
            *o = 0;
        }
        return;
    }

    if byte_shift > 0 {
        for index in 0..existing.len() {
            existing[index] = *existing.get(index + byte_shift).unwrap_or(&0);
        }
    }

    if bit_shift > 0 {
        let mut carry = 0;
        for i in (0..existing.len()).rev() {
            let next_carry = (existing[i] & ((1 << bit_shift) - 1)) << (8 - bit_shift);
            existing[i] = (existing[i] >> bit_shift) | carry;
            carry = next_carry;
        }
    }
}

#[allow(dead_code)]
#[inline]
fn left_shift(existing: &mut [u8], shift: u32) {
    let byte_shift = shift as usize / 8;
    let bit_shift = shift as usize % 8;

    if shift as usize >= existing.len() * 8 {
        for o in existing {
            *o = 0;
        }
        return;
    }

    if byte_shift > 0 {
        for index in (0..existing.len()).rev() {
            let shifted = if index < byte_shift {
                0
            } else {
                existing[index - byte_shift]
            };
            existing[index] = shifted;
        }
    }

    if bit_shift > 0 {
        let mut carry = 0;
        for i in (0..existing.len()).rev() {
            let next_carry =
                (existing[i] & (((1 << bit_shift) - 1) << (8 - bit_shift))) >> bit_shift;
            existing[i] = (existing[i] << bit_shift) | carry;
            carry = next_carry;
        }
    }
}

#[inline]
fn mask_off(existing: &mut [u8], high_bit: u32) -> &mut [u8] {
    let top_byte = high_bit as usize / 8;
    let top_bit = high_bit % 8;

    if top_byte < existing.len() {
        existing[top_byte] &= (1 << top_bit) - 1;
    }

    let len = existing.len();
    &mut existing[0..(top_byte + 1).min(len)]
}

/// Take a value and place it onto the existing value shifting by, `lower` and masking off at `upper`
fn write_modify(existing: &mut [u8], value: &[u8], lower: u32, upper: u32) {
    assert!(upper >= lower);
    assert!(existing.len() * 8 > upper as usize);

    let mut shift_count = upper - lower + 1;
    let mut read_ptr = 0;
    let mut write_ptr = lower / 8;
    let write_shift = lower % 8;

    let mut first_time_lengthen = write_shift;

    let mut carry = existing[write_ptr as usize] & ((1 << write_shift) - 1);
    while shift_count > 0 {
        let to_write = (value.get(read_ptr as usize).copied().unwrap_or(0) << write_shift) | carry;
        if write_shift > 0 {
            carry = (value.get(read_ptr as usize).copied().unwrap_or(0) >> (8 - write_shift))
                & ((1 << write_shift) - 1);
        }

        let write_count = shift_count.min(8 - first_time_lengthen) as u16;
        let write_mask = ((1 << (write_count + first_time_lengthen as u16).min(8)) - 1) as u8;

        first_time_lengthen = 0;

        existing[write_ptr as usize] =
            (to_write & write_mask) | (existing[write_ptr as usize] & !write_mask);

        read_ptr += 1;
        write_ptr += 1;

        shift_count -= write_count as u32;
    }
}

fn read_modify(existing: &mut [u8], lower: u32, upper: u32) -> &[u8] {
    assert!(upper >= lower);
    assert!(existing.len() * 8 > upper as usize);

    right_shift(existing, lower);
    &*mask_off(existing, upper - lower + 1)
}

/// These functions can' be stored as a fat pointer so they are split out here.
/// There is a blanket implementation for all types that implement HlComms.
pub trait HlCommsInterface: HlComms {
    fn axi_translate(&self, addr: impl AsRef<str>) -> Result<AxiData, AxiError> {
        let (arc_if, _) = self.comms_obj();

        arc_if.axi_translate(addr.as_ref())
    }

    fn axi_read_field<'a>(
        &self,
        addr: &AxiData,
        value: &'a mut [u8],
    ) -> Result<&'a [u8], PlatformError> {
        let (arc_if, chip_if) = self.comms_obj();

        if value.len() < addr.size as usize {
            return Err(AxiError::ReadBufferTooSmall)?;
        }

        arc_if.axi_read(chip_if, addr.addr, &mut value[..addr.size as usize])?;

        let value = if let Some((lower, upper)) = addr.bits {
            read_modify(value, lower, upper);

            value
        } else {
            &mut value[..addr.size as usize]
        };

        Ok(&*value)
    }

    fn axi_write_field(&self, addr: &AxiData, value: &[u8]) -> Result<(), PlatformError> {
        let (arc_if, chip_if) = self.comms_obj();

        if value.len() < addr.size as usize {
            return Err(AxiError::ReadBufferTooSmall)?;
        }

        if let Some((lower, upper)) = addr.bits {
            let mut existing = vec![0u8; addr.size as usize];
            arc_if.axi_read(chip_if, addr.addr, &mut existing)?;

            write_modify(&mut existing, value, lower, upper);

            arc_if.axi_write(chip_if, addr.addr, &existing)?;
        } else {
            // We are writing the full size of the field
            arc_if.axi_write(chip_if, addr.addr, &value[..addr.size as usize])?;
        };

        Ok(())
    }

    fn axi_sread<'a>(
        &self,
        addr: impl AsRef<str>,
        value: &'a mut [u8],
    ) -> Result<&'a [u8], PlatformError> {
        let (arc_if, _chip_if) = self.comms_obj();

        let addr = addr.as_ref();
        let addr = arc_if.axi_translate(addr)?;

        self.axi_read_field(&addr, value)
    }

    fn axi_sread_to_vec(&self, addr: impl AsRef<str>) -> Result<Vec<u8>, PlatformError> {
        let (arc_if, chip_if) = self.comms_obj();

        let addr = addr.as_ref();

        let addr = arc_if.axi_translate(addr)?;

        let mut output = Vec::with_capacity(addr.size as usize);

        let value: &mut [u8] = unsafe { std::mem::transmute(output.spare_capacity_mut()) };

        arc_if.axi_read(chip_if, addr.addr, &mut value[..addr.size as usize])?;

        unsafe {
            output.set_len(addr.size as usize);
        }

        Ok(output)
    }

    fn axi_sread32(&self, addr: impl AsRef<str>) -> Result<u32, PlatformError> {
        let mut value = [0; 4];

        let value = self.axi_sread(addr, &mut value)?;

        let mut output = 0;
        for o in value.iter().rev() {
            output <<= 8;
            output |= *o as u32;
        }

        Ok(output)
    }

    fn axi_swrite(&self, addr: impl AsRef<str>, value: &[u8]) -> Result<(), PlatformError> {
        let (arc_if, _chip_if) = self.comms_obj();

        let addr = arc_if.axi_translate(addr.as_ref())?;

        self.axi_write_field(&addr, value)
    }

    fn axi_swrite32(&self, addr: impl AsRef<str>, value: u32) -> Result<(), PlatformError> {
        self.axi_swrite(addr, &value.to_le_bytes())
    }
}

impl<T: HlComms> HlCommsInterface for T {}

#[cfg(test)]
mod test {
    #[test]
    fn test_read_modify() {
        let mut a = [0, 1, 2, 3];

        let a = super::read_modify(&mut a, 0, 31);

        assert_eq!(a, vec![0, 1, 2, 3]);
    }

    #[test]
    fn test_read_modify_bit() {
        let mut a = [0, 1, 2, 3];

        let a = super::read_modify(&mut a, 8, 8);

        assert_eq!(a, &[1]);
    }

    #[test]
    fn test_read_modify_bits() {
        let mut a = vec![0, 1, 2, 3];

        let a = super::read_modify(&mut a, 19, 25);

        assert_eq!(a, &[96]);
    }

    #[test]
    fn test_read_modify_top() {
        let mut a = [0, 0, 0, 0x80];

        let a = super::read_modify(&mut a, 31, 31);

        assert_eq!(a, vec![1]);
    }

    #[test]
    fn test_read_modify_bottom() {
        let mut a = [0x1, 0, 0, 0];

        let a = super::read_modify(&mut a, 0, 0);

        assert_eq!(a, vec![1]);
    }

    #[test]
    fn test_write_modify() {
        let mut a = vec![0, 1, 2, 3];
        let b = vec![0b110];

        super::write_modify(&mut a, &b, 0, 31);

        assert_eq!(a, vec![6, 0, 0, 0]);
    }

    #[test]
    fn test_write_modify_bit_low() {
        let mut a = vec![0, 1, 2, 3];
        let b = vec![0b0];

        // Check that we won't do anything to a low bit
        super::write_modify(&mut a, &b, 16, 16);
        assert_eq!(a, vec![0, 1, 2, 3]);

        super::write_modify(&mut a, &b, 24, 24);
        assert_eq!(a, vec![0, 1, 2, 2]);
    }

    #[test]
    fn test_write_modify_bit_high() {
        let mut a = vec![0, 1, 2, 3];
        let b = vec![0b1];

        // Check that we won't do anything to a high bit
        super::write_modify(&mut a, &b, 25, 25);
        assert_eq!(a, vec![0, 1, 2, 3]);

        super::write_modify(&mut a, &b, 18, 18);

        assert_eq!(a, vec![0, 1, 6, 3]);
    }

    #[test]
    fn test_write_modify_bits() {
        let mut a = vec![0, 1, 2, 3];
        let b = vec![0b110];

        super::write_modify(&mut a, &b, 13, 19);

        assert_eq!(a, vec![0, 193, 0, 3]);
    }

    #[test]
    fn test_write_modify_top() {
        let mut a = vec![0, 0, 0, 0];
        let b = vec![0b1];

        super::write_modify(&mut a, &b, 31, 31);

        assert_eq!(a, vec![0, 0, 0, 0x80]);
    }
}
