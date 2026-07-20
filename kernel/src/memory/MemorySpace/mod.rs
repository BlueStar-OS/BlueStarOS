//! 地址空间（Memory Space）子系统。
//!
//! 本目录由原 `memset.rs` 单文件按职责拆分而来，负责“进程/内核地址空间”的
//! 全部抽象。各文件分工：
//!
//! | 文件 | 职责 |
//! |------|------|
//! | `flags` | 位标志与映射类型：`MapAreaFlags`/`MmapProt`/`MmapFlags`/`CloneFlags`/`MapType` |
//! | `vir_num_range` | 虚拟页号闭区间 `VirNumRange` 及其迭代器 |
//! | `kernel_stack` | 内核栈 id 分配器 `KernelStackAllocator` |
//! | `mmap_entry` | mmap 区域按页记账 `MmapEntry` / `MmapEntryInfo` |
//! | `map_area` | 单个映射区域 `MapArea`（建/拆映射、拷数据） |
//! | `map_set` | 地址空间容器 `MapSet`（ELF/内核构建、fork、mmap/munmap/mprotect） |
//!
//! 对外通过 `pub use` 重新导出全部公有项，保持与拆分前 `crate::memory::*` 完全
//! 一致的使用路径，调用方无需改动。

mod flags;
mod kernel_stack;
mod map_area;
mod map_set;
mod mmap_entry;
mod vir_num_range;

pub use flags::*;
pub use kernel_stack::*;
pub use map_area::*;
pub use map_set::*;
pub use mmap_entry::*;
pub use vir_num_range::*;
