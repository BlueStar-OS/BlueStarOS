///wrapping ext4 block device
mod ext4;

use alloc::sync::Arc;
use rsext4::BlockDevice as RsExt4BlockDevice;
use crate::fs::vfs::{File, VfsFsError};

pub use ext4::*;


pub struct Ext4BlockDevice(pub Arc<dyn File>);

impl Ext4BlockDevice {
    pub fn new(dev: Arc<dyn File>) -> Self {
        Self(dev)
    }
}

impl RsExt4BlockDevice for Ext4BlockDevice {
    fn block_size(&self) -> u32 {
        crate::config::BLOCKSIZE as u32
    }
    fn close(&mut self) -> rsext4::BlockDevResult<()> {
        Ok(())
    }
    fn flush(&mut self) -> rsext4::BlockDevResult<()> {
        Ok(())
    }
    fn is_open(&self) -> bool {
        true
    }
    fn is_readonly(&self) -> bool {
        false
    }
    fn open(&mut self) -> rsext4::BlockDevResult<()> {
        Ok(())
    }
    fn read(&mut self, buffer: &mut [u8], block_id: u32, count: u32) -> rsext4::BlockDevResult<()> {
        let block_size = crate::config::BLOCKSIZE;
        let need = (count as usize)
            .checked_mul(block_size)
            .ok_or(rsext4::BlockDevError::IoError)?;
        if buffer.len() < need {
            return Err(rsext4::BlockDevError::IoError);
        }

        for blk in 0..(count as usize) {
            let off = blk * block_size;
            let sub = &mut buffer[off..off + block_size];
            let byte_off = ((block_id as usize) + blk)
                .checked_mul(block_size)
                .ok_or(rsext4::BlockDevError::IoError)?;
            let got = self
                .0
                .read_at(byte_off, sub)
                .map_err(|_| rsext4::BlockDevError::IoError)?;
            if got != block_size {
                return Err(rsext4::BlockDevError::IoError);
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
    fn write(&mut self, buffer: &[u8], block_id: u32, count: u32) -> rsext4::BlockDevResult<()> {
        let block_size = crate::config::BLOCKSIZE;
        let need = (count as usize)
            .checked_mul(block_size)
            .ok_or(rsext4::BlockDevError::IoError)?;
        if buffer.len() < need {
            return Err(rsext4::BlockDevError::IoError);
        }

        for blk in 0..(count as usize) {
            let off = blk * block_size;
            let sub = &buffer[off..off + block_size];
            let byte_off = ((block_id as usize) + blk)
                .checked_mul(block_size)
                .ok_or(rsext4::BlockDevError::IoError)?;
            let put = self
                .0
                .write_at(byte_off, sub)
                .map_err(|_| rsext4::BlockDevError::IoError)?;
            if put != block_size {
                return Err(rsext4::BlockDevError::IoError);
            }
        }
        Ok(())
    }
}