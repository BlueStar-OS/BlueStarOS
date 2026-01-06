use alloc::sync::Arc;
use spin::Mutex;
use spin::Mutex as SpinMutex;

use crate::driver::VirtBlk;
use crate::fs::partition::DevicePartition;
use crate::fs::vfs::{File, VfsFsError, VfsStat, VFS_DT_REG};
use crate::SECTOR_SIZE;
///BLOCK_DEV
pub struct VBLOCK{
    blockdevice:Arc<Mutex<VirtBlk>>,
    partition:DevicePartition,
    offset: SpinMutex<u64>,
}

impl VBLOCK {
    pub fn new(blockdevice: Arc<Mutex<VirtBlk>>, partition: DevicePartition) -> Self {
        Self {
            blockdevice,
            partition,
            offset: SpinMutex::new(0),
        }
    }

    fn part_base_lba(&self) -> u64 {
        self.partition.base_lba()
    }

    fn part_sectors(&self) -> u64 {
        self.partition.sectors()
    }

    fn part_len_bytes(&self) -> u64 {
        self.part_sectors().saturating_mul(SECTOR_SIZE as u64)
    }
}


impl File for VBLOCK {
    fn getdents64(&self, _max_len: usize) -> Result<alloc::vec::Vec<u8>, VfsFsError> {
        Err(VfsFsError::NotSupported)
    }

    fn read(&self, buf: &mut [u8]) -> Result<usize, VfsFsError> {
        let off = *self.offset.lock() as usize;
        let n = self.read_at(off, buf)?;
        *self.offset.lock() = off.saturating_add(n) as u64;
        Ok(n)
    }

    fn read_at(&self, offset: usize, buf: &mut [u8]) -> Result<usize, VfsFsError> {
        if buf.is_empty() {
            return Ok(0);
        }

        let part_len = self.part_len_bytes() as usize;
        if offset >= part_len {
            return Ok(0);
        }

        let mut remaining = core::cmp::min(buf.len(), part_len - offset);
        let mut written = 0usize;

        while remaining > 0 {
            let abs_off = (self.part_base_lba() as usize)
                .saturating_mul(SECTOR_SIZE)
                .saturating_add(offset + written);
            let lba = abs_off / SECTOR_SIZE;
            let in_off = abs_off % SECTOR_SIZE;
            let to_copy = core::cmp::min(remaining, SECTOR_SIZE - in_off);

            let mut sector: [u8; SECTOR_SIZE] = [0; SECTOR_SIZE];
            self.blockdevice
                .lock()
                .0
                .lock()
                .read_block(lba, &mut sector)
                .map_err(|_| VfsFsError::IO)?;

            buf[written..written + to_copy]
                .copy_from_slice(&sector[in_off..in_off + to_copy]);
            written += to_copy;
            remaining -= to_copy;
        }

        Ok(written)
    }

    fn stat(&self) -> Result<VfsStat, VfsFsError> {
        Ok(VfsStat {
            inode: 0,
            size: self.part_len_bytes(),
            mode: 0,
            file_type: VFS_DT_REG,
        })
    }

    fn write(&self, buf: &[u8]) -> Result<usize, VfsFsError> {
        let off = *self.offset.lock() as usize;
        let n = self.write_at(off, buf)?;
        *self.offset.lock() = off.saturating_add(n) as u64;
        Ok(n)
    }

    fn write_at(&self, offset: usize, buf: &[u8]) -> Result<usize, VfsFsError> {
        if buf.is_empty() {
            return Ok(0);
        }

        let part_len = self.part_len_bytes() as usize;
        if offset >= part_len {
            return Ok(0);
        }

        let mut remaining = core::cmp::min(buf.len(), part_len - offset);
        let mut read_pos = 0usize;

        while remaining > 0 {
            let abs_off = (self.part_base_lba() as usize)
                .saturating_mul(SECTOR_SIZE)
                .saturating_add(offset + read_pos);
            let lba = abs_off / SECTOR_SIZE;
            let in_off = abs_off % SECTOR_SIZE;
            let to_copy = core::cmp::min(remaining, SECTOR_SIZE - in_off);

            let mut sector: [u8; SECTOR_SIZE] = [0; SECTOR_SIZE];
            if to_copy != SECTOR_SIZE {
                self.blockdevice
                    .lock()
                    .0
                    .lock()
                    .read_block(lba, &mut sector)
                    .map_err(|_| VfsFsError::IO)?;
            }

            sector[in_off..in_off + to_copy]
                .copy_from_slice(&buf[read_pos..read_pos + to_copy]);
            self.blockdevice
                .lock()
                .0
                .lock()
                .write_block(lba, &sector)
                .map_err(|_| VfsFsError::IO)?;

            read_pos += to_copy;
            remaining -= to_copy;
        }

        Ok(read_pos)
    }

    fn lseek(&self, offset: isize, whence: usize) -> Result<usize, VfsFsError> {
        let cur = *self.offset.lock() as i64;
        let end = self.part_len_bytes() as i64;
        let next = match whence {
            0 => offset as i64,
            1 => cur.saturating_add(offset as i64),
            2 => end.saturating_add(offset as i64),
            _ => return Err(VfsFsError::Invalid),
        };
        if next < 0 {
            return Err(VfsFsError::Invalid);
        }
        *self.offset.lock() = next as u64;
        Ok(next as usize)
    }
}