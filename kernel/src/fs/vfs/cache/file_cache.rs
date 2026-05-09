//! 单个文件页缓存对象。
//!
//! 这一层只负责“一页数据本身”：
//! - 从 backing file 读入一页；
//! - 和物理页帧之间做双向拷贝；
//! - 在需要时把脏页前缀刷回文件。
//!
//! 上层索引与全局查找由 `cacheblkmanager.rs` 负责。

use crate::arch::memory::PhysiAddr;
use crate::config::PAGE_SIZE;
use crate::fs::vfs::{File, VfsFsError};
use crate::memory::FramTracker;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

/// 文件页的页对齐 offset。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct FilePageOffset(pub usize);

/// 单个缓存页。
pub struct CachedFilePage {
    backing_file: Arc<dyn File>,
    page_offset: FilePageOffset,
    page_bytes: Vec<u8>,
    is_dirty: bool,
}

impl CachedFilePage {
    /// 从文件中读入一整页，未读满部分补 0。
    pub fn load_from_file(
        backing_file: Arc<dyn File>,
        page_offset: FilePageOffset,
    ) -> Result<Self, VfsFsError> {
        let mut page_bytes = vec![0; PAGE_SIZE];
        let read_bytes = backing_file.read_at(page_offset.0, &mut page_bytes)?;
        if read_bytes < PAGE_SIZE {
            page_bytes[read_bytes..].fill(0);
        }

        Ok(Self {
            backing_file,
            page_offset,
            page_bytes,
            is_dirty: false,
        })
    }

    /// 把缓存页内容复制到物理页帧中。
    pub fn copy_into_frame(&self, frame: &Arc<FramTracker>) {
        let physical_addr: PhysiAddr = frame.ppn.into();
        let frame_bytes =
            unsafe { core::slice::from_raw_parts_mut(physical_addr.0 as *mut u8, PAGE_SIZE) };
        frame_bytes.copy_from_slice(&self.page_bytes);
    }

    /// 把物理页帧内容复制回缓存页，并标记 dirty。
    pub fn copy_from_frame(&mut self, frame: &Arc<FramTracker>) {
        let physical_addr: PhysiAddr = frame.ppn.into();
        let frame_bytes =
            unsafe { core::slice::from_raw_parts(physical_addr.0 as *const u8, PAGE_SIZE) };
        self.page_bytes.copy_from_slice(frame_bytes);
        self.is_dirty = true;
    }

    /// 把缓存页前缀刷回文件。
    ///
    /// TODO(dirinkbottle):
    /// 对“文件最后一个不足一页的共享映射”来说，当前仍沿用你原来的策略：
    /// 只写回 `write_len` 字节，而不是整页。这样最稳，但还没有做更完整的
    /// 脏区间追踪。
    pub fn flush_prefix_to_file(&mut self, write_len: usize) -> Result<(), VfsFsError> {
        let write_len = write_len.min(PAGE_SIZE);
        if write_len == 0 || !self.is_dirty {
            return Ok(());
        }

        self.backing_file
            .write_at(self.page_offset.0, &self.page_bytes[..write_len])?;
        self.backing_file.flush()?;
        self.is_dirty = false;
        Ok(())
    }
}
