use crate::{chip::AxiData, error::PlatformError};

use super::Blackhole;

#[derive(Debug, thiserror::Error)]
pub enum MessageError {
    #[error("Timed out in {phase} after {}s", .timeout.as_secs_f32())]
    Timeout {
        phase: String,
        timeout: std::time::Duration,
    },
    #[error("Selected out of range queue ({index} > {queue_count})")]
    QueueIndexOutOfRange { index: u32, queue_count: u32 },
}

#[derive(Clone)]
pub struct MessageQueue<const N: usize> {
    pub header_size: u32,
    pub entry_size: u32,

    pub queue_base: u64,
    pub queue_count: u32,

    pub queue_size: u32,

    pub fw_int: AxiData,
}

impl<const N: usize> MessageQueue<N> {
    fn get_base(&self, index: u8) -> u64 {
        let msg_queue_size = 2 * self.queue_size * (self.entry_size * 4) + (self.header_size * 4);
        self.queue_base + (index as u64 * msg_queue_size as u64)
    }

    fn qread32(
        &self,
        chip: &Blackhole,
        index: u8,
        offset: u32,
    ) -> Result<u32, Box<dyn std::error::Error>> {
        chip.arc_if
            .axi_read32(&chip.chip_if, self.get_base(index) + (4 * offset as u64))
    }

    fn qwrite32(
        &self,
        chip: &Blackhole,
        index: u8,
        offset: u32,
        value: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        chip.arc_if.axi_write32(
            &chip.chip_if,
            self.get_base(index) + (4 * offset as u64),
            value,
        )
    }

    fn trigger_int(&self, chip: &Blackhole) -> Result<bool, PlatformError> {
        let mut mvalue = vec![0u8; self.fw_int.size as usize];
        let value =
            crate::chip::HlCommsInterface::axi_read_field(&chip, &self.fw_int, &mut mvalue)?;

        if value[0] & 1 != 0 {
            return Ok(false);
        }

        mvalue[0] |= 1;

        crate::chip::HlCommsInterface::axi_write_field(&chip, &self.fw_int, mvalue.as_slice())?;

        Ok(true)
    }

    fn push_request(
        &self,
        chip: &Blackhole,
        index: u8,
        request: &[u32; N],
        timeout: std::time::Duration,
    ) -> Result<(), PlatformError> {
        let request_queue_wptr = self.qread32(chip, index, 0)?;

        let start_time = std::time::Instant::now();
        loop {
            let request_queue_rptr = self.qread32(chip, index, 4)?;

            // Check if the queue is full
            if request_queue_rptr.abs_diff(request_queue_wptr) % (2 * self.queue_size)
                != self.queue_size
            {
                break;
            }

            let elapsed = start_time.elapsed();
            if elapsed > timeout {
                return Err(MessageError::Timeout {
                    phase: "push".to_string(),
                    timeout: elapsed,
                })?;
            }
        }

        let request_entry_offset =
            self.header_size + (request_queue_wptr % self.queue_size) * N as u32;
        for (i, &item) in request.iter().enumerate() {
            self.qwrite32(chip, index, request_entry_offset + i as u32, item)?;
        }

        let request_queue_wptr = (request_queue_wptr + 1) % (2 * self.queue_size);
        self.qwrite32(chip, index, 0, request_queue_wptr)?;

        self.trigger_int(chip)?;

        Ok(())
    }

    fn pop_response(
        &self,
        chip: &Blackhole,
        index: u8,
        result: &mut [u32; N],
        timeout: std::time::Duration,
    ) -> Result<(), PlatformError> {
        let response_queue_rptr = self.qread32(chip, index, 1)?;

        let start_time = std::time::Instant::now();
        loop {
            let response_queue_wptr = self.qread32(chip, index, 5)?;

            // Break if there is some data in the queue
            if response_queue_wptr != response_queue_rptr {
                break;
            }

            let elapsed = start_time.elapsed();
            if elapsed > timeout {
                return Err(MessageError::Timeout {
                    phase: "pop".to_string(),
                    timeout: elapsed,
                })?;
            }
        }

        let response_entry_offset = self.header_size
            + (self.queue_size + (response_queue_rptr % self.queue_size)) * N as u32;
        for (i, item) in result.iter_mut().enumerate() {
            *item = self.qread32(chip, index, response_entry_offset + i as u32)?;
        }

        let response_queue_rptr = (response_queue_rptr + 1) % (2 * self.queue_size);
        self.qwrite32(chip, index, 1, response_queue_rptr)?;

        Ok(())
    }

    pub fn send_message(
        &self,
        chip: &Blackhole,
        index: u8,
        mut request: [u32; N],
        timeout: std::time::Duration,
    ) -> Result<[u32; N], PlatformError> {
        if index as u32 > self.queue_count {
            return Err(MessageError::QueueIndexOutOfRange {
                index: index as u32,
                queue_count: self.queue_count,
            })?;
        }

        self.push_request(chip, index, &request, timeout)?;
        self.pop_response(chip, index, &mut request, timeout)?;

        Ok(request)
    }
}

#[derive(Debug)]
pub struct QueueInfo {
    pub req_rptr: u32,
    pub req_wptr: u32,
    pub resp_rptr: u32,
    pub resp_wptr: u32,
}

impl<const N: usize> MessageQueue<N> {
    pub fn get_queue_info(&self, chip: &Blackhole, index: u8) -> Result<QueueInfo, PlatformError> {
        if index as u32 > self.queue_count {
            return Err(MessageError::QueueIndexOutOfRange {
                index: index as u32,
                queue_count: self.queue_count,
            })?;
        }

        Ok(QueueInfo {
            req_rptr: self.qread32(chip, index, 4)?,
            req_wptr: self.qread32(chip, index, 0)?,
            resp_rptr: self.qread32(chip, index, 1)?,
            resp_wptr: self.qread32(chip, index, 5)?,
        })
    }
}
