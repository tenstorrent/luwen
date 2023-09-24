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

/// These functions can' be stored as a fat pointer so they are split out here.
/// There is a blanket implementation for all types that implement HlComms.
pub trait HlCommsInterface: HlComms {
    fn axi_translate(&self, addr: impl AsRef<str>) -> Result<AxiData, AxiError> {
        let (arc_if, _) = self.comms_obj();

        arc_if.axi_translate(addr.as_ref())
    }

    fn axi_sread<'a>(
        &self,
        addr: impl AsRef<str>,
        value: &'a mut [u8],
    ) -> Result<&'a [u8], PlatformError> {
        let (arc_if, chip_if) = self.comms_obj();

        let addr = addr.as_ref();

        let addr = arc_if.axi_translate(addr)?;

        if value.len() < addr.size as usize {
            return Err(AxiError::ReadBufferTooSmall)?;
        }

        arc_if.axi_read(chip_if, addr.addr, &mut value[..addr.size as usize])?;

        Ok(&value[..addr.size as usize])
    }

    fn axi_sread_to_vec(&self, addr: impl AsRef<str>) -> Result<Vec<u8>, PlatformError> {
        let (arc_if, chip_if) = self.comms_obj();

        let addr = addr.as_ref();

        let addr = arc_if.axi_translate(addr)?;

        let mut output = Vec::with_capacity(addr.size as usize);

        let value: &mut [u8] = unsafe { std::mem::transmute(output.spare_capacity_mut()) };

        arc_if.axi_read(chip_if, addr.addr, &mut value[..addr.size as usize])?;

        Ok(output)
    }

    fn axi_sread32(&self, addr: impl AsRef<str>) -> Result<u32, PlatformError> {
        let mut output = [0; 4];

        self.axi_sread(addr, &mut output)?;

        Ok(u32::from_le_bytes(output))
    }

    fn axi_swrite(&self, addr: impl AsRef<str>, value: &[u8]) -> Result<(), PlatformError> {
        let (arc_if, chip_if) = self.comms_obj();

        let addr = arc_if.axi_translate(addr.as_ref())?;

        if value.len() != addr.size as usize {
            return Err(AxiError::WriteBufferMismatch)?;
        }

        arc_if.axi_write(chip_if, addr.addr, &value[..addr.size as usize])?;

        Ok(())
    }

    fn axi_swrite32(&self, addr: impl AsRef<str>, value: u32) -> Result<(), PlatformError> {
        self.axi_swrite(addr, &value.to_le_bytes())
    }
}

impl<T: HlComms> HlCommsInterface for T {}
