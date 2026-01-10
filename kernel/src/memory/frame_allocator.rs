use log::{trace, warn};
use crate::{config::{KERNEL_HEADP, KERNEL_HEAP_SIZE, MB, PAGE_SIZE}, memory::address::*,sync::UPSafeCell};
use core::cell::UnsafeCell;
use buddy_system_allocator::LockedHeap;
#[allow(static_mut_refs)]
use lazy_static::lazy_static;

#[global_allocator]
pub static ALLOCATOR: LockedHeap = LockedHeap::empty(); //内核堆分配器
use alloc::vec::Vec;

pub fn allocator_init(){
    unsafe{
        #[allow(static_mut_refs)]
        let start = KERNEL_HEADP.as_ptr() as usize;
        let end = start + KERNEL_HEAP_SIZE;
        use log::info;
        info!("heap range: [{:#x}, {:#x}) size={} MB", start, end, KERNEL_HEAP_SIZE/MB);
        #[allow(static_mut_refs)]
        ALLOCATOR.lock().init(KERNEL_HEADP.as_ptr() as usize,KERNEL_HEAP_SIZE);
    }
    trace!("Kernel HeapAlloctor init, can use size:{}MB , mount on KERNEL_HEADP",KERNEL_HEAP_SIZE/MB);
}

///物理页分配器 [start,end)
pub struct FrameAlloctor{
    ///代表起始物理页号
    start:usize,
    ///辅助记录初始start
    origin:usize,
    ///代表结束物理页号，不能取
    end:usize,
    ///页帧回收池
    recycle:Vec<usize>
}

trait FrameAllocatorTrait{
    fn new()->Self;
    fn alloc(&mut self)->Option<FramTracker>;
    fn dealloc(&mut self,ppn:usize);
}
impl FrameAllocatorTrait for FrameAlloctor{
    fn new()->Self {
        FrameAlloctor{
            start:0,
            end:0,
            origin:0,
            recycle:Vec::new()
        }
    }
    ///分配物理页帧
    fn alloc(&mut self)->Option<FramTracker>{
        if let Some(ppn)=self.recycle.pop(){
            trace!("realloc frame:ppn:{}",ppn);
            Some(FramTracker::new(PhysiNumber(ppn)))
        }else if self.start<self.end{
            let ppn=self.start;
            self.start+=1;
            trace!("alloc frame:ppn:{}",ppn);
            Some(FramTracker::new(PhysiNumber(ppn)))
        }else{
            panic!("no more frame!");
        }
    }

    ///回收物理页帧
    fn dealloc(&mut self,ppn:usize) {
        //页号合法性检查
        if ppn<self.origin || ppn>= self.start || ppn>self.end {
            panic!("frame ppn:{} is not valid! orign:{} start:{} end:{} ",ppn,self.origin,self.start,self.end);
        }
        if self.recycle.contains(&ppn) {
            //双重释放
            panic!("Please note this, if this not a share cause double free please check");
        }else {
            trace!("Frame ppn: {} was recycled!",ppn);
            //回收物理页帧
            self.recycle.push(ppn);
        }
        
    }

}

impl FrameAlloctor {
    pub fn init(&mut self,start:usize,end:usize){
        self.start=PhysiAddr(start).floor_up().0;
        self.end=PhysiAddr(end).floor_down().0;
        self.recycle=Vec::new(); 
        self.origin=PhysiAddr(start).floor_up().0;
        trace!("frame allocator init: start ppn:{} end ppn:{} size:{}MB",self.start,self.end,(end-start)/MB);
    }
}


#[derive(Debug,Clone)]
pub struct FramTracker{
    pub ppn:PhysiNumber
}
impl FramTracker{
    fn new(ppn:PhysiNumber)->Self{
        unsafe {
            let addr: PhysiAddr = ppn.into();
            // 清洗旧数据，确保新分配的帧是干净的
            core::slice::from_raw_parts_mut(addr.0 as *mut u8, PAGE_SIZE).fill(0);
        }
        FramTracker{
            ppn
        }
    }
}
lazy_static!{
    pub static ref FRAME_ALLOCATOR:UPSafeCell<FrameAlloctor>= 
    unsafe {
        UPSafeCell::new(FrameAlloctor::new())
    };
}
pub fn init_frame_allocator(start:usize,end:usize){
    FRAME_ALLOCATOR.lock().init(start,end);
}
pub fn alloc_frame()->Option<FramTracker>{
    FRAME_ALLOCATOR.lock().alloc()
}

pub fn alloc_contiguous_frames(pages: usize) -> Option<Vec<FramTracker>> {
    FRAME_ALLOCATOR.lock().alloc_contiguous(pages)
}

pub fn dealloc_frame(ppn:usize){
    FRAME_ALLOCATOR.lock().dealloc(ppn);
}

impl Drop for FramTracker {
    fn drop(&mut self) {
        dealloc_frame(self.ppn.0);
        //trace!("free frame:ppn:{}",self.ppn.0);
    }
}

impl FrameAlloctor {
    /// Allocate `pages` physically contiguous frames from the bump region.
    ///
    /// This is primarily used for DMA buffers (e.g. VirtIO queues) which
    /// require physical contiguity. It intentionally does NOT allocate from
    /// the recycle list, because recycled frames are not guaranteed to be
    /// contiguous.
    pub fn alloc_contiguous(&mut self, pages: usize) -> Option<Vec<FramTracker>> {
        if pages == 0 {
            return Some(Vec::new());
        }
        if self.start + pages > self.end {
            return None;
        }
        let base = self.start;
        self.start += pages;
        let mut v = Vec::with_capacity(pages);
        for i in 0..pages {
            v.push(FramTracker::new(PhysiNumber(base + i)));
        }
        Some(v)
    }
}
