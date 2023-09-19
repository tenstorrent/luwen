use super::{AxiError, ChipComms, ChipInterface};

/// Convinence trait for high-level communication with an arbitrary chip.
pub trait HlComms {
    fn comms_obj(&self) -> (&dyn ChipComms, &dyn ChipInterface);

    fn noc_read32(&self, noc_id: u8, x: u8, y: u8, addr: u64) -> u32 {
        let (arc_if, chip_if) = self.comms_obj();
        arc_if.noc_read32(chip_if, noc_id, x, y, addr)
    }

    fn noc_write32(&self, noc_id: u8, x: u8, y: u8, addr: u64, value: u32) {
        let (arc_if, chip_if) = self.comms_obj();
        arc_if.noc_write32(chip_if, noc_id, x, y, addr, value)
    }

    fn noc_broadcast32(&self, noc_id: u8, addr: u64, value: u32) {
        let (arc_if, chip_if) = self.comms_obj();
        arc_if.noc_broadcast32(chip_if, noc_id, addr, value)
    }

    fn axi_read32(&self, addr: u64) -> u32 {
        let (arc_if, chip_if) = self.comms_obj();
        arc_if.axi_read32(chip_if, addr)
    }

    fn axi_write32(&self, addr: u64, value: u32) {
        let (arc_if, chip_if) = self.comms_obj();
        arc_if.axi_write32(chip_if, addr, value)
    }

    fn axi_sread32(&self, addr: impl AsRef<str>) -> Result<u32, AxiError> {
        let (arc_if, chip_if) = self.comms_obj();
        arc_if.axi_sread32(chip_if, addr.as_ref())
    }

    fn axi_swrite32(&self, addr: impl AsRef<str>, value: u32) -> Result<(), AxiError> {
        let (arc_if, chip_if) = self.comms_obj();
        arc_if.axi_swrite32(chip_if, addr.as_ref(), value)
    }
}
