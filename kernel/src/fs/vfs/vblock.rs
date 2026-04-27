//! 块设备抽象与块设备文件。
//!
//! ## 两层设计
//!
//! | 层级 | 类型 | 职责 |
//! |------|------|------|
//! | 设备层 | `BlockDevTrait` | 裸扇区读写——驱动实现 |
//! | 文件层 | `BLOCKDEVFILE` | 包装设备 + 线性映射，对外暴露 VFS `File` |
//!
//! `BLOCKDEVFILE` 使用 `DmlinerEntry` 做 LBA 偏移翻译，
//! 使得同一个 backing device 上的不同区域可以表现为多个独立的
//! 块设备文件（`/dev/vda`、`/dev/vda1`、`/dev/vda2` …）。

use alloc::sync::Arc;
use spin::Mutex;
use spin::Mutex as SpinMutex;

use crate::fs::vfs::dm_liner::DmlinerEntry;
use crate::fs::vfs::{File, FileOffset, VfsFsError, VfsStat, VFS_DT_REG};
use crate::SECTOR_SIZE;

// ── BlockDevTrait ─────────────────────────────────────────────

/// 块设备能力接口。
///
/// 任何物理或虚拟块设备（VirtIO-blk、eMMC、dm-linear）都实现此 trait。
/// 方法使用 `&mut self`，调用方通过外层 `Mutex` 保证独占访问。
pub trait BlockDevTrait: Send + Sync {
    /// 读取一个扇区到 `buf`。
    ///
    /// `lba`: 逻辑扇区号。`buf.len()` 必须 >= `SECTOR_SIZE`。
    fn read_block(&mut self, lba: usize, buf: &mut [u8]) -> Result<(), VfsFsError>;

    /// 将 `buf` 写入一个扇区。
    ///
    /// `lba`: 逻辑扇区号。`buf.len()` 必须 >= `SECTOR_SIZE`。
    fn write_block(&mut self, lba: usize, buf: &[u8]) -> Result<(), VfsFsError>;

    /// 设备总扇区数。
    fn capacity_in_sectors(&self) -> u64;
}

// ── BLOCKDEVFILE ──────────────────────────────────────────────

/// 块设备文件：将一个 `BlockDevTrait` 的一片连续扇区暴露为 VFS `File`。
///
/// ## 字段
///
/// | 字段 | 作用 |
/// |------|------|
/// | `blockdevice` | 底层物理/虚拟块设备（多实例共享同一 `Arc`） |
/// | `lineer_info` | 在 backing device 上的起始扇区和扇区数 |
/// | `offset` | 文件光标——顺序 `read`/`write`/`lseek` 依赖它 |
///
/// `BLOCKDEVFILE` 同时支持随机读写（`read_at`/`write_at`）
/// 和顺序读写（`read`/`write`/`lseek`）。随机读写不触动 `offset`。
pub struct BLOCKDEVFILE {
    /// 底层块设备句柄（与同一设备上的其他 BLOCKDEVFILE 共享）
    blockdevice: Arc<Mutex<dyn BlockDevTrait>>,
    /// 本文件对应的扇区区间
    lineer_info: DmlinerEntry,
    /// 当前文件读写光标（字节偏移）
    offset: SpinMutex<FileOffset>,
}

impl BLOCKDEVFILE {
    /// 创建一个块设备文件。
    ///
    /// `lineer_info.start_lba` 为 0 时表示从设备起始地址开始
    ///（整盘设备节点如 `/dev/vda`）；非零时表示一个分区或
    /// dm-linear 卷（如 `/dev/vda1`、`/dev/my_vg/my_lv`）。
    pub fn new(
        blockdevice: Arc<Mutex<dyn BlockDevTrait>>,
        lineer_info: DmlinerEntry,
    ) -> Self {
        Self {
            blockdevice,
            lineer_info,
            offset: SpinMutex::new(FileOffset(0)),
        }
    }

    /// backing device 上本区间的起始扇区号。
    fn part_base_lba(&self) -> u64 {
        self.lineer_info.start_lba
    }

    /// 本区间包含的扇区数。
    fn part_sectors(&self) -> u64 {
        self.lineer_info.sectors
    }

    /// 本区间的总字节数。
    fn part_len_bytes(&self) -> u64 {
        self.part_sectors().saturating_mul(SECTOR_SIZE as u64)
    }
}

// ── File impl ─────────────────────────────────────────────────

impl File for BLOCKDEVFILE {
    fn getdents64(&self, _max_len: usize) -> Result<alloc::vec::Vec<u8>, VfsFsError> {
        Err(VfsFsError::NotSupported)
    }

    /// 从当前光标顺序读取，读后推进光标。
    fn read(&self, buf: &mut [u8]) -> Result<usize, VfsFsError> {
        let off = self.offset.lock().0 as usize;
        let n = self.read_at(off, buf)?;
        *self.offset.lock() = FileOffset(off.saturating_add(n) as u64);
        Ok(n)
    }

    /// 从指定字节偏移读取，不影响光标。
    ///
    /// 内部将字节偏移翻译为 (LBA, 扇区内偏移)，逐扇区读取并拼接。
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
            self.blockdevice.lock().read_block(lba, &mut sector)?;

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

    /// 从当前光标顺序写入，写后推进光标。
    fn write(&self, buf: &[u8]) -> Result<usize, VfsFsError> {
        let off = self.offset.lock().0 as usize;
        let n = self.write_at(off, buf)?;
        *self.offset.lock() = FileOffset(off.saturating_add(n) as u64);
        Ok(n)
    }

    /// 从指定字节偏移写入，不影响光标。
    ///
    /// 对应扇区如果不是整扇区写入，需要先读出旧内容做
    /// read-modify-write，避免覆盖同扇区其他数据。
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
                // 非整扇区写入：先读出旧扇区内容，修改目标区间后写回
                self.blockdevice.lock().read_block(lba, &mut sector)?;
            }

            sector[in_off..in_off + to_copy]
                .copy_from_slice(&buf[read_pos..read_pos + to_copy]);
            self.blockdevice.lock().write_block(lba, &sector)?;

            read_pos += to_copy;
            remaining -= to_copy;
        }

        Ok(read_pos)
    }

    /// 调整文件光标。
    ///
    /// `whence`: 0=文件头, 1=当前位置, 2=文件尾。返回新的字节偏移。
    fn lseek(&self, offset: isize, whence: usize) -> Result<usize, VfsFsError> {
        let cur = self.offset.lock().0 as i64;
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
        *self.offset.lock() = FileOffset(next as u64);
        Ok(next as usize)
    }
}
