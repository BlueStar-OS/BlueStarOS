use alloc::vec::Vec;

mod frame_allocator;
pub mod memorymodel;
// 目录名沿用 `MemorySpace`（用户约定），但模块标识用 snake_case 以满足
// `#![deny(non_snake_case)]`；通过 `#[path]` 把二者衔接起来。
#[path = "MemorySpace/mod.rs"]
mod memory_space;

pub use frame_allocator::*;
pub use memorymodel::*;
pub use memory_space::*;

use crate::sync::UPSafeCell;
use lazy_static::lazy_static;
pub struct PreFlectMemory {
    pub range: VirNumRange,
    pub flags: MapAreaFlags,
}

impl PreFlectMemory {
    pub fn new(range: VirNumRange, flags: MapAreaFlags) -> Self {
        Self { range, flags }
    }
}

// 内核地址空间区域注册，用于早期使用dtb_probe探针将需要映射的内核内存都插入进去
lazy_static! {
    pub static ref KERNEL_MEMORY_SPACE_LIST: UPSafeCell<Vec<PreFlectMemory>> =
        UPSafeCell::new(Vec::new());
}

/// 注册内核 MMIO 区域（供 dtb_probe 回调使用）
pub fn register_kernel_mmio(range: VirNumRange, flags: MapAreaFlags) {
    use crate::arch::memory::VirAddr;
    let start_addr: VirAddr = range.0.into();
    let end_addr: VirAddr = range.1.into();
    kprintln!(
        "[MMIO Registry] Registers: {:#x}-{:#x}",
        start_addr.0,
        end_addr.0
    );
    KERNEL_MEMORY_SPACE_LIST.lock(|list| list.push(PreFlectMemory::new(range, flags)));
}
