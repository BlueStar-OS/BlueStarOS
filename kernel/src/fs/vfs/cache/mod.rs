//! VFS 文件页缓存。
//!
//! 当前先提供 mmap 需要的最小能力：
//! - 按 `(file, page_offset)` 缓存文件页；
//! - page fault 时从缓存填充物理页；
//! - `MAP_SHARED` 解除映射/地址空间销毁时，将页帧内容同步回缓存并刷盘。

mod cacheblkmanager;
mod file_cache;

