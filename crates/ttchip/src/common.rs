use kmdif::{PciDevice, PciError, PciOpenError};
use luwen_core::Arch;
use thiserror::Error;

use crate::axi::{Axi, AxiError, AxiReadWrite};

pub trait DeviceTransport {
    fn read_mapping(&self, address: &str) -> u32;
    fn write_mapping(&self, address: &str, value: u32);

    fn block_read_mapping(&self, address: &str, value: &mut [u8]);
    fn block_write_mapping(&self, address: &str, value: &[u8]);

    fn read32(&self, address: usize) -> u32;
    fn write32(&self, address: usize, value: u32);
}

pub struct PciTransport {
    is_interface: bool,
}

impl DeviceTransport for PciTransport {
    fn read_mapping(&self, address: &str) -> u32 {
        if self.is_interface {
            todo!()
        } else {
            todo!()
        }
    }

    fn write_mapping(&self, address: &str, value: u32) {
        if self.is_interface {
            todo!()
        } else {
            todo!()
        }
    }

    fn block_read_mapping(&self, address: &str, value: &mut [u8]) {
        todo!()
    }

    fn block_write_mapping(&self, address: &str, value: &[u8]) {
        todo!()
    }

    fn read32(&self, address: usize) -> u32 {
        todo!()
    }

    fn write32(&self, address: usize, value: u32) {
        todo!()
    }
}

pub struct Chip {
    pub transport: kmdif::PciDevice,
    pub axi: Axi,
}

impl Chip {
    pub fn arch(&self) -> Arch {
        self.transport.arch.clone()
    }
}

pub enum ArcMsg {
    TEST { arg: u32 },
    ARC_GO_TO_SLEEP,
}

impl ArcMsg {
    pub fn msg_code(&self) -> u16 {
        let code = match self {
            ArcMsg::ARC_GO_TO_SLEEP => 0x55,
            ArcMsg::TEST { .. } => 0x90,
        };

        0xaa00 | code
    }
}

#[derive(Error, Debug)]
pub enum ArcMsgError {
    #[error("Message {0} not recognized")]
    MsgNotRecognized(u16),
    #[error("Timed out while waiting {0:?} for ARC to respond")]
    Timeout(std::time::Duration),
    #[error("ARC is asleep")]
    ArcAsleep,

    #[error(transparent)]
    PciError(#[from] PciError),

    #[error(transparent)]
    AxiError(#[from] AxiError),
}

pub enum ArcMsgOk {
    Ok(u32),
    OkNoWait,
}

impl Chip {
    pub fn create(device_id: usize) -> Result<Self, PciOpenError> {
        Ok(Self {
            transport: PciDevice::open(device_id)?,
            axi: Axi::empty(),
        })
    }

    pub fn axi(&mut self) -> AxiReadWrite {
        AxiReadWrite {
            axi: &self.axi,
            transport: &mut self.transport,
        }
    }

    /// Returns True if new interrupt triggered, or False if the
    /// FW is currently busy. The message IRQ handler should only take a couple
    /// dozen cycles, so if this returns False it probably means something went
    /// wrong.
    fn trigger_fw_int(&mut self) -> Result<bool, AxiError> {
        // let misc = self.transport.read32("ARC_RESET.ARC_MISC_CNTL")?;
        let misc = self.transport.read32(0x1FF30000 + 0x0100)?;

        if misc & (1 << 16) != 0 {
            return Ok(false);
        }

        let misc_bit16_set = misc | (1 << 16);
        self.axi()
            .write("ARC_RESET.ARC_MISC_CNTL", &misc_bit16_set.to_le_bytes())?;
        // self.transport
        //     .write32(0x1FF30000 + 0x0100, misc_bit16_set)?;

        Ok(true)
    }

    pub fn arc_msg(
        &mut self,
        msg: &mut ArcMsg,
        wait_for_done: bool,
        timeout: std::time::Duration,
        use_second_mailbox: bool,
    ) -> Result<ArcMsgOk, ArcMsgError> {
        const MSG_ERROR_REPLY: u32 = 0xffffffff;

        let arg0: u16;
        let arg1: u16;
        match &msg {
            ArcMsg::TEST { arg } => {
                arg0 = (arg & 0xFFFF) as u16;
                arg1 = ((arg >> 16) & 0xFFFF) as u16;
            }
            ArcMsg::ARC_GO_TO_SLEEP => {
                arg0 = 0;
                arg1 = 0;
            }
        }

        let (msg_reg, return_reg) = if use_second_mailbox { (2, 4) } else { (5, 3) };

        let code = msg.msg_code();

        // let current_code = self.transport.read32("ARC_RESET.SCRATCH[{msg_reg}]");
        let current_code = self.transport.read32(0x1FF30000 + 0x0060 + 4 * msg_reg)?;
        if (current_code & 0xFFFF) as u16 == ArcMsg::ARC_GO_TO_SLEEP.msg_code() {
            return Err(ArcMsgError::ArcAsleep);
        }

        // self.transport.write32(
        //     0x1FF30000 + 0x0060 + 4 * return_reg,
        //     arg0 as u32 | ((arg1 as u32) << 16),
        // )?;
        self.axi().write(
            &format!("ARC_RESET.SCRATCH[{return_reg}]"),
            &(arg0 as u32 | ((arg1 as u32) << 16)).to_le_bytes(),
        )?;

        // self.transport
        //     .write32(0x1FF30000 + 0x0060 + 4 * msg_reg, code as u32)?;
        self.axi().write(
            &format!("ARC_RESET.SCRATCH[{msg_reg}]"),
            &(code as u32).to_le_bytes(),
        )?;

        assert!(self.trigger_fw_int()?);

        if wait_for_done {
            let start = std::time::Instant::now();
            loop {
                // let status: u32 = self.transport.read32(0x1FF30000 + 0x0060 + 4 * msg_reg)?;
                let status: u32 = self.axi().read(&format!("ARC_RESET.SCRATCH[{msg_reg}]"))?;
                if (status & 0xFFFF) as u16 == code & 0xFF {
                    let exit_code = (status >> 16) & 0xFFFF;
                    return Ok(ArcMsgOk::Ok(exit_code));
                } else if status == MSG_ERROR_REPLY {
                    return Err(ArcMsgError::MsgNotRecognized(code));
                }

                std::thread::sleep(std::time::Duration::from_millis(1));
                if start.elapsed() > timeout {
                    return Err(ArcMsgError::Timeout(timeout));
                }
            }
        }

        Ok(ArcMsgOk::OkNoWait)
    }
}
