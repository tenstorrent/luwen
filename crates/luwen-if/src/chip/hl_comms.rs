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

/// Take a value and place it onto the existing value shifting by, `lower` and masking off at `upper`
fn write_modify0(existing: &mut [u8], value: &[u8], lower: u32, upper: u32) {
    let (array_shift, element_shift) = if lower == 0 {
        (0, 0)
    } else {
        (lower / 8, lower % 8)
    };

    let (upper_shift, upper_element_shift) = if upper == 0 {
        (0, 0)
    } else {
        (upper / 8, upper % 8)
    };

    // We can shift off the lower elements for "free"
    // let mut existing = vec![0u8; (value.len() - array_shift as u64) as usize];
    // arc_if.axi_read(chip_if, addr.addr + array_shift as u64, &mut existing)?;

    assert!(existing.len() * 8 > upper as usize);
    assert!(upper >= lower);

    let mut it = existing;

    // We are able to skip the bottom elements of existing
    if array_shift > 0 {
        it = &mut it[array_shift as usize..]
    }

    // We are doing both modifications with a bitmask
    if upper_shift - array_shift == 0 {
        it[array_shift as usize] =
            (value[0] & (1 << (upper_element_shift - element_shift)) - 1) << element_shift;
    } else {
        // We are doing a byte modification before (and after our bit mod)
        let mut carry = it[0] & ((1 << element_shift) - 1);
        let carry_mask = ((1 << element_shift) - 1) << (8 - element_shift);

        let byte_count = (upper_shift - array_shift) as usize;

        for i in 0..byte_count {
            // This is our bytemod, for this region the existing value will be equal to the "value"
            // shifted up and the carray from the previous shift in the lower bits. For the first
            // iteration we make the carry equal to the lower bits of the existing value to emulate
            // a rmw operation.
            it[i] = (value[i] << element_shift) | carry;
            carry = (value[i] & carry_mask) >> (8 - element_shift);
        }

        // For the final bitop we just do our normal setup, but mask off at the upper shift. And
        // perform a mw to the existing value
        let bitmask = (1 << upper_element_shift) - 1;
        let value = value[byte_count] << element_shift | carry;
        it[byte_count] = (it[byte_count] & !bitmask) | (value & bitmask);
    }
}

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
        let to_write =
            (value.get(read_ptr as usize).map(|v| *v).unwrap_or(0) << write_shift) | carry;
        if write_shift > 0 {
            carry = (value.get(read_ptr as usize).map(|v| *v).unwrap_or(0) >> (8 - write_shift))
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

    let mut shift_count = upper - lower + 1;
    let mut read_ptr = lower / 8;
    let read_shift = lower % 8;
    let mut write_ptr = 0;
    while shift_count > 0 {
        let mut to_write = existing[read_ptr as usize] >> read_shift;
        if read_shift > 0 {
            to_write |= existing
                .get((read_ptr + 1) as usize)
                .map(|v| *v)
                .unwrap_or(0)
                << (8 - read_shift);
        }
        let write_count = shift_count.min(8) as u16;
        let write_mask = ((1 << write_count) - 1) as u8;

        existing[write_ptr as usize] = to_write & write_mask;

        read_ptr += 1;
        write_ptr += 1;

        shift_count -= write_count as u32;
    }

    &existing[..write_ptr]
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

        self.axi_write_field(&addr, &value)
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
}
