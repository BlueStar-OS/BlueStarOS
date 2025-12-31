///wrapping ext4 block device
mod inode;
mod ext4;
pub mod ext4test;

use crate::driver::{VirtBlk};
use rsext4::BlockDevice as RsExt4BlockDevice;

pub use ext4::*;
pub use inode::*;

pub struct Ext4BlockDevice(VirtBlk);

impl Ext4BlockDevice {
    pub fn new(dev: VirtBlk) -> Self {
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
        const SECTOR_SIZE: usize = 512;
        let block_size = crate::config::BLOCKSIZE;
        if block_size % SECTOR_SIZE != 0 {
            return Err(rsext4::BlockDevError::InvalidBlockSize {
                size: block_size,
                expected: SECTOR_SIZE,
            });
        }

        let sectors_per_block = (block_size / SECTOR_SIZE) as u64;
        for blk in 0..(count as u64) {
            for sec in 0..sectors_per_block {
                let sector_id = (block_id as u64 + blk) * sectors_per_block + sec;
                let off = ((blk * sectors_per_block + sec) as usize) * SECTOR_SIZE;
                let buf = &mut buffer[off..off + SECTOR_SIZE];
                self.0
                    .0
                    .lock()
                    .read_block(sector_id as usize, buf)
                    .map_err(|_| rsext4::BlockDevError::IoError)?;
            }
        }
        Ok(())
    }
    fn total_blocks(&self) -> u64 {
        // VirtIOBlk 的 capacity() 返回扇区数（512字节/扇区）
        // 需要转换为块数（BLOCKSIZE字节/块）
        let capacity_in_sectors = self.0.capacity_in_sectors();
        let sector_size = 512u64;
        let block_size = crate::config::BLOCKSIZE as u64;
        
        // 计算总块数 = (总扇区数 * 扇区大小) / 块大小
        (capacity_in_sectors * sector_size) / block_size
    }
    fn write(&mut self, buffer: &[u8], block_id: u32, count: u32) -> rsext4::BlockDevResult<()> {
        const SECTOR_SIZE: usize = 512; //以底层块设备扇区为单位
        let block_size = crate::config::BLOCKSIZE;
        if block_size % SECTOR_SIZE != 0 {
            return Err(rsext4::BlockDevError::InvalidBlockSize {
                size: block_size,
                expected: SECTOR_SIZE,
            });
        }

        let sectors_per_block = (block_size / SECTOR_SIZE) as u64;
        for blk in 0..(count as u64) {
            for sec in 0..sectors_per_block {
                let sector_id = (block_id as u64 + blk) * sectors_per_block + sec;
                let off = ((blk * sectors_per_block + sec) as usize) * SECTOR_SIZE;
                let buf = &buffer[off..off + SECTOR_SIZE];
                self.0
                    .0
                    .lock()
                    .write_block(sector_id as usize, buf)
                    .map_err(|_| rsext4::BlockDevError::IoError)?;
            }
        }
        Ok(())
    }
}