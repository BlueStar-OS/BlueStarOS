//! 存储从DTB扫描的物理内存区域信息
//! dtb::init() 扫描 memory 节点后注册到 KERNEL_MAIN_MEMORY

use alloc::vec::Vec;
use lazy_static::lazy_static;
use crate::sync::UPSafeCell;
use crate::config::MB;

/// 一段物理内存区域（物理地址）
#[derive(Debug, Clone, Copy)]
pub struct PhysMemoryRange {
    pub start: usize, // 物理起始地址
    pub end: usize,   // 物理结束地址（不含）
}

impl PhysMemoryRange {
    pub fn size(&self) -> usize {
        self.end.saturating_sub(self.start)
    }
}

/// 机器物理内存信息（支持多段不连续内存）
pub struct MachineMemoryInfo {
    regions: Vec<PhysMemoryRange>,
    initialized: bool,
}

impl MachineMemoryInfo {
    fn new() -> Self {
        Self {
            regions: Vec::new(),
            initialized: false,
        }
    }

    /// 注册一段物理内存区域
    pub fn register(&mut self, start: usize, end: usize) {
        kprintln!("[MemModel] Register Physical Memory region: {:#x}-{:#x} ({} MB)",
               start, end, (end - start) / MB);
        self.regions.push(PhysMemoryRange { start, end });
    }

    pub fn mark_initialized(&mut self) {
        self.initialized = true;
    }

    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    pub fn regions(&self) -> &[PhysMemoryRange] {
        &self.regions
    }

    pub fn total_size(&self) -> usize {
        self.regions.iter().map(|r| r.size()).sum()
    }
}

lazy_static! {
    pub static ref KERNEL_MAIN_MEMORY: UPSafeCell<MachineMemoryInfo> =
        unsafe { UPSafeCell::new(MachineMemoryInfo::new()) };
}

/// 注册一段物理内存区域（供 dtb::init 调用）
pub fn register_main_memory_region(start: usize, end: usize) {
    KERNEL_MAIN_MEMORY.lock().register(start, end);
}

/// 标记物理内存注册完成
pub fn finalize_main_memory() {
    KERNEL_MAIN_MEMORY.lock().mark_initialized();
}
