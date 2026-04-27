///wrapping ext4 block device
mod ext4;

use crate::arch::time::ext4_current_time;
use crate::fs::vfs::File;
use alloc::sync::Arc;
use rsext4::bmalloc::AbsoluteBN;
use rsext4::BlockDevice as RsExt4BlockDevice;

pub use ext4::*;

pub struct Ext4BlockDevice(pub Arc<dyn File>);

impl Ext4BlockDevice {
    pub fn new(dev: Arc<dyn File>) -> Self {
        Self(dev)
    }
}

fn ext4_io_error() -> rsext4::Ext4Error {
    rsext4::Ext4Error::io()
}

impl RsExt4BlockDevice for Ext4BlockDevice {
    fn block_size(&self) -> u32 {
        crate::config::BLOCKSIZE as u32
    }
    fn close(&mut self) -> rsext4::Ext4Result<()> {
        Ok(())
    }
    fn flush(&mut self) -> rsext4::Ext4Result<()> {
        Ok(())
    }
    fn is_open(&self) -> bool {
        true
    }
    fn is_readonly(&self) -> bool {
        false
    }
    fn open(&mut self) -> rsext4::Ext4Result<()> {
        Ok(())
    }
    fn read(
        &mut self,
        buffer: &mut [u8],
        block_id: AbsoluteBN,
        count: u32,
    ) -> rsext4::Ext4Result<()> {
        let block_size = crate::config::BLOCKSIZE;
        let need = (count as usize)
            .checked_mul(block_size)
            .ok_or_else(ext4_io_error)?;
        if buffer.len() < need {
            return Err(rsext4::Ext4Error::buffer_too_small(buffer.len(), need));
        }

        for blk in 0..(count as usize) {
            let off = blk * block_size;
            let sub = &mut buffer[off..off + block_size];
            let byte_off = block_id
                .checked_add_usize(blk)?
                .as_usize()?
                .checked_mul(block_size)
                .ok_or_else(ext4_io_error)?;
            let got = self
                .0
                .read_at(byte_off, sub)
                .map_err(|_| rsext4::Ext4Error::io())?;
            if got != block_size {
                return Err(rsext4::Ext4Error::io());
            }
        }
        Ok(())
    }
    fn total_blocks(&self) -> u64 {
        let block_size = crate::config::BLOCKSIZE as u64;
        let size = match self.0.stat() {
            Ok(s) => s.size,
            Err(_) => 0,
        };
        size / block_size
    }
    fn write(&mut self, buffer: &[u8], block_id: AbsoluteBN, count: u32) -> rsext4::Ext4Result<()> {
        let block_size = crate::config::BLOCKSIZE;
        let need = (count as usize)
            .checked_mul(block_size)
            .ok_or_else(ext4_io_error)?;
        if buffer.len() < need {
            return Err(rsext4::Ext4Error::buffer_too_small(buffer.len(), need));
        }
        for blk in 0..(count as usize) {
            let off = blk * block_size;
            let sub = &buffer[off..off + block_size];
            let byte_off = block_id
                .checked_add_usize(blk)?
                .as_usize()?
                .checked_mul(block_size)
                .ok_or_else(ext4_io_error)?;
            let put = self
                .0
                .write_at(byte_off, sub)
                .map_err(|_| rsext4::Ext4Error::io())?;
            if put != block_size {
                return Err(rsext4::Ext4Error::io());
            }
        }
        Ok(())
    }

    fn current_time(&self) -> rsext4::Ext4Result<rsext4::Ext4Timestamp> {
        Ok(ext4_current_time())
    }
}
