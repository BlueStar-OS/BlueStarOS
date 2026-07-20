//! `mmap` 区域的按页记账信息。
//!
//! [`MmapEntry`] 保存整个 mmap 区域的语义标志与一张“vpn -> 页信息”表；
//! [`MmapEntryInfo`] 描述单页的懒分配来源（匿名页或文件页）、所绑定的物理页帧
//! （用 `Weak` 持有，避免与 `MapArea::frames` 的强引用形成环）以及页级权限。
//! 这里集中处理 fork 拷贝、缺页填充（`read_into_frame`）与 MAP_SHARED 回写
//! （`write_back_from_frame`）等页级操作。

use super::flags::{MmapFlags, MmapProt};
use crate::arch::memory::PhysiAddr;
use crate::config::PAGE_SIZE;
use crate::fs::vfs::File;
use crate::memory::frame_allocator::FramTracker;
use alloc::collections::btree_map::BTreeMap;
use alloc::sync::{Arc, Weak};

use crate::arch::memory::VirNumber;

/// 单次mmap条目
#[derive(Clone)]
pub struct MmapEntry {
    /// 整个 mmap 区域的语义（SHARED/PRIVATE/FIXED/ANONYMOUS）
    pub flags: MmapFlags,
    /// mmap表
    pub mmap_tree: BTreeMap<VirNumber, MmapEntryInfo>,
}

impl MmapEntry {
    pub fn new() -> Self {
        MmapEntry {
            flags: MmapFlags::empty(),
            mmap_tree: BTreeMap::new(),
        }
    }

    pub fn with_flags(flags: MmapFlags) -> Self {
        MmapEntry {
            flags,
            mmap_tree: BTreeMap::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.mmap_tree.is_empty()
    }

    pub fn contains_vpn(&self, vpn: VirNumber) -> bool {
        self.mmap_tree.contains_key(&vpn)
    }

    pub fn get(&self, vpn: VirNumber) -> Option<&MmapEntryInfo> {
        self.mmap_tree.get(&vpn)
    }

    pub fn get_mut(&mut self, vpn: VirNumber) -> Option<&mut MmapEntryInfo> {
        self.mmap_tree.get_mut(&vpn)
    }

    pub fn clone_for_fork(&self) -> Self {
        let mut new_entry = MmapEntry::with_flags(self.flags);
        for (vpn, info) in self.mmap_tree.iter() {
            let cloned = if self.flags.contains(MmapFlags::SHARED) {
                info.clone()
            } else {
                info.clone_without_frame()
            };
            new_entry.mmap_tree.insert(*vpn, cloned);
        }
        new_entry
    }
}

#[derive(Clone)]
pub enum MmapEntryInfo {
    Anoy {
        // 映射到哪个物理页帧上
        frame: Weak<FramTracker>,
        //映射权限
        prot: MmapProt,
    },
    File {
        file: Arc<dyn File>,
        // 文件映射起始偏移
        offset: usize,
        // 文件映射长度
        mmap_len: usize,
        // 映射到哪个物理页帧上
        frame: Weak<FramTracker>,
        // 映射权限
        prot: MmapProt,
    },
}

impl MmapEntryInfo {
    pub(crate) fn clone_without_frame(&self) -> Self {
        match self {
            Self::Anoy { prot, .. } => Self::Anoy {
                frame: Weak::new(),
                prot: *prot,
            },
            Self::File {
                file,
                offset,
                mmap_len,
                prot,
                ..
            } => Self::File {
                file: file.clone(),
                offset: *offset,
                mmap_len: *mmap_len,
                frame: Weak::new(),
                prot: *prot,
            },
        }
    }

    pub(crate) fn upgrade_frame(&self) -> Option<Arc<FramTracker>> {
        match self {
            Self::Anoy { frame, .. } | Self::File { frame, .. } => frame.upgrade(),
        }
    }

    pub(crate) fn set_frame(&mut self, new_frame: &Arc<FramTracker>) {
        match self {
            Self::Anoy { frame, .. } | Self::File { frame, .. } => {
                *frame = Arc::downgrade(new_frame);
            }
        }
    }

    pub(crate) fn read_into_frame(
        &self,
        frame: &Arc<FramTracker>,
    ) -> Result<(), crate::fs::vfs::VfsFsError> {
        let pa: PhysiAddr = frame.ppn.into();
        let buf = unsafe { core::slice::from_raw_parts_mut(pa.0 as *mut u8, PAGE_SIZE) };
        buf.fill(0);

        match self {
            Self::Anoy { .. } => Ok(()),
            Self::File {
                file,
                offset,
                mmap_len,
                ..
            } => {
                let to_read = (*mmap_len).min(PAGE_SIZE);
                if to_read == 0 {
                    return Ok(());
                }
                match file.read_at(*offset, &mut buf[..to_read]) {
                    Ok(n) => {
                        if n < to_read {
                            buf[n..to_read].fill(0);
                        }
                        Ok(())
                    }
                    Err(err) => Err(err),
                }
            }
        }
    }

    pub(crate) fn write_back_from_frame(
        &self,
        frame: &Arc<FramTracker>,
    ) -> Result<(), crate::fs::vfs::VfsFsError> {
        let Self::File {
            file,
            offset,
            mmap_len,
            ..
        } = self
        else {
            return Ok(());
        };

        let to_write = (*mmap_len).min(PAGE_SIZE);
        if to_write == 0 {
            return Ok(());
        }

        let pa: PhysiAddr = frame.ppn.into();
        let buf = unsafe { core::slice::from_raw_parts(pa.0 as *const u8, to_write) };
        file.write_at(*offset, buf).map(|_| ())
    }
}
