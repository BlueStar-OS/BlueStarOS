//! 内核栈虚拟地址区间的 id 分配器。
//!
//! 每个任务都需要一段独立的内核栈，本文件负责“逻辑 id”的分配/回收：
//! [`KernelStackAllocator`] 维护一个递增计数器和回收队列，`MapSet` 的
//! `alloc_kernel_stack` 等方法据此在高地址区域切分出互不重叠的内核栈区间。
//! 全局单例 [`KERNEL_STACK_ALLOCATOR`] 供同模块的 `map_set` 使用。

use crate::sync::UPSafeCell;
use alloc::vec::Vec;
use lazy_static::lazy_static;
use log::error;

pub struct KernelStackAllocator {
    pub(crate) current_id: usize,
    pub(crate) recycle: Vec<usize>,
}

impl KernelStackAllocator {
    fn new() -> Self {
        Self {
            current_id: 0,
            recycle: Vec::new(),
        }
    }

    pub(crate) fn alloc_id(&mut self) -> usize {
        if let Some(id) = self.recycle.pop() {
            id
        } else {
            let id = self.current_id;
            self.current_id += 1;
            id
        }
    }
    pub fn dealloc_id(&mut self, id: usize) {
        if self.recycle.contains(&id) {
            error!("Kernel stack id recycle error,it has in recycle list")
        }
        self.recycle.push(id);
    }
}

lazy_static! {
    pub(crate) static ref KERNEL_STACK_ALLOCATOR: UPSafeCell<KernelStackAllocator> =
        UPSafeCell::new(KernelStackAllocator::new());
}
