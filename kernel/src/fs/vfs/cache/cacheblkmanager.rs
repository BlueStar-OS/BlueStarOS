//! 最小文件页缓存管理器。
//!
//! 这一版只解决当前 `mmap` 最缺的那部分能力：
//! 1. 以 `(文件对象, 页对齐 offset)` 作为 key 缓存单页数据；
//! 2. page fault 时优先从缓存装页，未命中时再调用 `read_at()`；
//! 3. `MAP_SHARED` 回写时，先把页帧内容同步回缓存，再把缓存页刷到文件。
//!
//! 设计参考 Linux 5.4.29：
//! - `mm/filemap.c:9-12`：普通文件系统的 mmap 统一走 generic file mmap 语义；
//! - `mm/filemap.c:2454-2536`：page fault 优先查 page cache，未命中才补页。
//!
//! TODO(dirinkbottle):
//! - 还没有做 LRU / 容量上限 / reclaim；
//! - 还没有处理 truncate / unlink / close 后的失效；
//! - 还没有做脏页聚合写回，目前仍是最小同步写回模型。

use super::file_cache::{CachedFilePage, FilePageOffset};
use crate::config::PAGE_SIZE;
use crate::fs::vfs::{File, VfsFsError};
use crate::memory::FramTracker;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use lazy_static::lazy_static;
use spin::Mutex;

/// 以文件对象的数据指针作为缓存身份。
///
/// 这里不尝试做 inode 级统一身份，而是先用“当前打开文件对象”作为最小 key。
/// 对当前 `mmap(fd, ...)` 路径来说，这已经足够把同一 file handle 的重复 fault
/// 合并掉。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct FileCacheIdentity(usize);

impl FileCacheIdentity {
    /// 从 `Arc<dyn File>` 提取一个稳定身份。
    fn from_file(backing_file: &Arc<dyn File>) -> Self {
        let raw_file_ptr: *const dyn File = Arc::as_ptr(backing_file);
        Self(raw_file_ptr as *const () as usize)
    }
}

/// 全局文件页缓存 key。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct FileCachePageKey {
    file_identity: FileCacheIdentity,
    page_offset: FilePageOffset,
}

impl FileCachePageKey {
    fn new(backing_file: &Arc<dyn File>, page_offset: FilePageOffset) -> Self {
        Self {
            file_identity: FileCacheIdentity::from_file(backing_file),
            page_offset,
        }
    }
}

/// 最小文件页缓存索引表。
pub struct FilePageCacheManager {
    cached_pages: BTreeMap<FileCachePageKey, Arc<Mutex<CachedFilePage>>>,
}

impl FilePageCacheManager {
    fn new() -> Self {
        Self {
            cached_pages: BTreeMap::new(),
        }
    }

    /// 获取指定文件页；若未命中，则从文件中读入并插入缓存。
    fn get_or_load_page(
        &mut self,
        backing_file: Arc<dyn File>,
        page_offset: FilePageOffset,
    ) -> Result<Arc<Mutex<CachedFilePage>>, VfsFsError> {
        let cache_key = FileCachePageKey::new(&backing_file, page_offset);
        if let Some(cached_page) = self.cached_pages.get(&cache_key) {
            return Ok(cached_page.clone());
        }

        let new_cached_page = Arc::new(Mutex::new(CachedFilePage::load_from_file(
            backing_file,
            page_offset,
        )?));
        self.cached_pages.insert(cache_key, new_cached_page.clone());
        Ok(new_cached_page)
    }
}

lazy_static! {
    static ref FILE_PAGE_CACHE_MANAGER: Mutex<FilePageCacheManager> =
        Mutex::new(FilePageCacheManager::new());
}

/// 标准化页 offset：对齐到页边界。
fn file_page_offset(page_offset_bytes: usize) -> Result<FilePageOffset, VfsFsError> {
    if !page_offset_bytes.is_multiple_of(PAGE_SIZE) {
        return Err(VfsFsError::Invalid);
    }
    Ok(FilePageOffset(page_offset_bytes))
}

/// 统一拿到一个缓存页，再在回调中对其进行操作。
fn with_cached_file_page<T>(
    backing_file: &Arc<dyn File>,
    page_offset_bytes: usize,
    callback: impl FnOnce(&mut CachedFilePage) -> Result<T, VfsFsError>,
) -> Result<T, VfsFsError> {
    let page_offset = file_page_offset(page_offset_bytes)?;
    let cached_page = {
        let mut cache_manager = FILE_PAGE_CACHE_MANAGER.lock();
        cache_manager.get_or_load_page(backing_file.clone(), page_offset)?
    };

    let mut cached_page_guard = cached_page.lock();
    callback(&mut cached_page_guard)
}

/// 从文件页缓存把一页数据装进物理页帧。
///
/// 对 `mmap` page fault 来说，这对应 Linux `filemap_fault()` 的“先查 page cache，
/// 命中就直接拿页”的阶段，参考 `mm/filemap.c:2493-2536`。
pub fn populate_frame_from_file_page_cache(
    backing_file: &Arc<dyn File>,
    page_offset_bytes: usize,
    frame: &Arc<FramTracker>,
) -> Result<(), VfsFsError> {
    with_cached_file_page(backing_file, page_offset_bytes, |cached_page| {
        cached_page.copy_into_frame(frame);
        Ok(())
    })
}

/// 把物理页帧内容同步回文件页缓存，并标记为 dirty。
pub fn sync_frame_into_file_page_cache(
    backing_file: &Arc<dyn File>,
    page_offset_bytes: usize,
    frame: &Arc<FramTracker>,
) -> Result<(), VfsFsError> {
    with_cached_file_page(backing_file, page_offset_bytes, |cached_page| {
        cached_page.copy_from_frame(frame);
        Ok(())
    })
}

/// 把缓存页的前 `write_len` 字节刷回文件。
///
/// 当前只做最小同步写回，和你现在 `munmap()` / `MapSet::drop()` 的语义保持一致。
pub fn flush_file_page_cache(
    backing_file: &Arc<dyn File>,
    page_offset_bytes: usize,
    write_len: usize,
) -> Result<(), VfsFsError> {
    with_cached_file_page(backing_file, page_offset_bytes, |cached_page| {
        cached_page.flush_prefix_to_file(write_len)
    })
}
