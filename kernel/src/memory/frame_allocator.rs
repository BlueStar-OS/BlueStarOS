use crate::arch::memory::*;
use crate::{
    config::{KERNEL_HEAP_SIZE, MB, PAGE_SIZE},
    sync::UPSafeCell,
};
use buddy_system_allocator::LockedHeap;
use log::trace;

use lazy_static::lazy_static;

#[global_allocator]
pub static ALLOCATOR: LockedHeap = LockedHeap::empty(); //内核堆分配器
use alloc::vec::Vec;

pub fn allocator_init() {
    unsafe {
        use crate::config::KERNEL_HEADP;
        #[allow(static_mut_refs)]
        let start = KERNEL_HEADP.as_ptr() as usize;
        let end = start + KERNEL_HEAP_SIZE;
        kprintln!(
            "heap range: [{:#x}, {:#x}) size={} MB",
            start,
            end,
            KERNEL_HEAP_SIZE / MB
        );
        #[allow(static_mut_refs)]
        ALLOCATOR
            .lock()
            .init(KERNEL_HEADP.as_ptr() as usize, KERNEL_HEAP_SIZE);
    }
    kprintln!(
        "Kernel HeapAlloctor init, can use size:{}MB , mount on KERNEL_HEADP",
        KERNEL_HEAP_SIZE / MB
    );
}

/// 一段物理页号范围
#[derive(Debug, Clone)]
struct PageRange {
    /// 当前分配位置（bump pointer，PPN）
    current: usize,
    /// 起始页号（dealloc 合法性检查用）
    origin: usize,
    /// 结束页号（不含）
    end: usize,
}

impl PageRange {
    fn remaining(&self) -> usize {
        self.end.saturating_sub(self.current)
    }
}

/// 物理页分配器（支持多段不连续内存）
pub struct FrameAlloctor {
    /// 所有可用的物理页范围
    ranges: Vec<PageRange>,
    /// 当前正在分配的范围索引
    active: usize,
    /// 页帧回收池
    recycle: Vec<usize>,
}

trait FrameAllocatorTrait {
    fn new() -> Self;
    fn alloc_continue_frame(&mut self, page_count: usize) -> Option<Vec<FramTracker>>;
    fn alloc(&mut self) -> Option<FramTracker>;
    fn dealloc(&mut self, ppn: usize);
}

impl FrameAllocatorTrait for FrameAlloctor {
    fn new() -> Self {
        FrameAlloctor {
            ranges: Vec::new(),
            active: 0,
            recycle: Vec::new(),
        }
    }

    /// 分配单个物理页帧
    fn alloc(&mut self) -> Option<FramTracker> {
        // 优先从回收池分配
        if let Some(ppn) = self.recycle.pop() {
            return Some(FramTracker::new(PhysiNumber(ppn)));
        }
        // 遍历 ranges 找可用的
        while self.active < self.ranges.len() {
            let range = &mut self.ranges[self.active];
            if range.current < range.end {
                let ppn = range.current;
                range.current += 1;
                unsafe {
                    core::slice::from_raw_parts_mut((ppn * PAGE_SIZE) as *mut u8, PAGE_SIZE)
                        .fill(0);
                }
                return Some(FramTracker::new(PhysiNumber(ppn)));
            }
            // 当前 range 耗尽，移到下一个
            self.active += 1;
        }
        panic!("no more frame!");
    }

    /// 分配一段连续的物理页内存（从回收池搜索）
    fn alloc_continue_frame(&mut self, page_count: usize) -> Option<Vec<FramTracker>> {
        // 先从 bump 区域尝试（当前 active range）
        if self.active < self.ranges.len() {
            let range = &mut self.ranges[self.active];
            if range.remaining() >= page_count {
                let ppn = range.current;
                range.current += page_count;
                unsafe {
                    core::slice::from_raw_parts_mut(
                        (ppn * PAGE_SIZE) as *mut u8,
                        PAGE_SIZE * page_count,
                    )
                    .fill(0);
                }
                let mut frame_vec: Vec<FramTracker> = Vec::with_capacity(page_count);
                for i in 0..page_count {
                    frame_vec.push(FramTracker::new(PhysiNumber(ppn + i)));
                }
                return Some(frame_vec);
            }
        }
        // bump 区域不足，尝试从回收池找连续段
        if self.recycle.len() < page_count {
            panic!("no more frame!");
        }
        self.recycle.sort();
        let start_ppn = self
            .recycle
            .windows(page_count)
            .position(|window| window[page_count - 1] - window[0] == page_count - 1);
        let pos = match start_ppn {
            Some(pos) => pos,
            None => panic!("no more continue frame!"),
        };
        let base_ppn = self.recycle[pos];
        self.recycle.drain(pos..pos + page_count);
        let mut frame_vec: Vec<FramTracker> = Vec::with_capacity(page_count);
        for i in 0..page_count {
            frame_vec.push(FramTracker::new(PhysiNumber(base_ppn + i)));
        }
        Some(frame_vec)
    }

    /// 回收物理页帧
    fn dealloc(&mut self, ppn: usize) {
        // 检查 ppn 是否属于任何已注册的 range
        let valid = self.ranges.iter().any(|r| ppn >= r.origin && ppn < r.end);
        if !valid {
            panic!("frame ppn:{} is not valid in any range!", ppn);
        }
        if self.recycle.contains(&ppn) {
            panic!("double free detected for ppn:{}", ppn);
        }
        trace!("Frame ppn: {} was recycled!", ppn);
        self.recycle.push(ppn);
    }
}

impl FrameAlloctor {
    /// 从多段物理内存初始化
    pub fn init_ranges(&mut self, regions: &[(usize, usize)]) {
        self.ranges.clear();
        self.active = 0;
        self.recycle = Vec::new();

        for &(start_addr, end_addr) in regions {
            let start_ppn = PhysiAddr(start_addr).floor_up().0;
            let end_ppn = PhysiAddr(end_addr).floor_down().0;
            if end_ppn > start_ppn {
                kprintln!(
                    "frame range: ppn [{}, {}) addr [{:#x}, {:#x}) size={}MB",
                    start_ppn,
                    end_ppn,
                    start_addr,
                    end_addr,
                    (end_addr - start_addr) / MB
                );
                self.ranges.push(PageRange {
                    current: start_ppn,
                    origin: start_ppn,
                    end: end_ppn,
                });
            }
        }
        let total_pages: usize = self.ranges.iter().map(|r| r.end - r.origin).sum();
        kprintln!(
            "frame allocator init: {} ranges, total {} pages ({}MB)",
            self.ranges.len(),
            total_pages,
            total_pages * PAGE_SIZE / MB
        );
    }

    /// 单段初始化（兼容旧接口）
    pub fn init(&mut self, start: usize, end: usize) {
        self.init_ranges(&[(start, end)]);
    }

    /// DMA 用连续页分配（只从 bump 区域）
    pub fn alloc_contiguous(&mut self, pages: usize) -> Option<Vec<FramTracker>> {
        if pages == 0 {
            return Some(Vec::new());
        }
        for i in self.active..self.ranges.len() {
            let range = &mut self.ranges[i];
            if range.remaining() >= pages {
                let base = range.current;
                range.current += pages;
                if i > self.active {
                    self.active = i;
                }
                let mut v = Vec::with_capacity(pages);
                for j in 0..pages {
                    v.push(FramTracker::new(PhysiNumber(base + j)));
                }
                return Some(v);
            }
        }
        None
    }
}

#[derive(Debug, Clone)]
pub struct FramTracker {
    pub ppn: PhysiNumber,
}
impl FramTracker {
    fn new(ppn: PhysiNumber) -> Self {
        unsafe {
            let addr: PhysiAddr = ppn.into();
            core::slice::from_raw_parts_mut(addr.0 as *mut u8, PAGE_SIZE).fill(0);
        }
        FramTracker { ppn }
    }
}

lazy_static! {
    pub static ref FRAME_ALLOCATOR: UPSafeCell<FrameAlloctor> =
        unsafe { UPSafeCell::new(FrameAlloctor::new()) };
}

pub fn init_frame_allocator(start: usize, end: usize) {
    kprintln!("Init frame allocator start={:#x}, end={:#x}", start, end);
    FRAME_ALLOCATOR.lock().init(start, end);
}

/// 从 DTB 探测的物理内存初始化帧分配器
pub fn init_frame_allocator_from_dtb(kernel_end: usize) {
    use crate::memory::memorymodel::KERNEL_MAIN_MEMORY;

    let mem_info = KERNEL_MAIN_MEMORY.lock();

    if !mem_info.is_initialized() || mem_info.regions().is_empty() {
        panic!("DTB memory info is not avalible");
        // warn!("DTB内存信息不可用，使用MEMORY_SIZE回退: {}MB", MEMORY_SIZE / MB);
        // drop(mem_info);
        // FRAME_ALLOCATOR.lock().init(kernel_end, kernel_end + MEMORY_SIZE);
        // return;
    }

    // 构建 (start, end) 列表，跳过 kernel image 占用的页
    let mut regions: Vec<(usize, usize)> = Vec::new();
    for region in mem_info.regions() {
        if region.end <= kernel_end {
            continue; // 整个区域在 kernel image 之内
        }
        let effective_start = if region.start < kernel_end {
            kernel_end
        } else {
            region.start
        };
        if effective_start < region.end {
            regions.push((effective_start, region.end));
        }
    }
    drop(mem_info);

    if regions.is_empty() {
        panic!("没有可用的物理内存区域!");
    }

    FRAME_ALLOCATOR.lock().init_ranges(&regions);
}

pub fn alloc_frame() -> Option<FramTracker> {
    FRAME_ALLOCATOR.lock().alloc()
}

pub fn alloc_contiguous_frames(pages: usize) -> Option<Vec<FramTracker>> {
    FRAME_ALLOCATOR.lock().alloc_contiguous(pages)
}

pub fn dealloc_frame(ppn: usize) {
    FRAME_ALLOCATOR.lock().dealloc(ppn);
}

impl Drop for FramTracker {
    fn drop(&mut self) {
        dealloc_frame(self.ppn.0);
    }
}
