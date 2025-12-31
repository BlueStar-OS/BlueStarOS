use virtio_drivers::{Hal, VirtIOBlk, VirtIOHeader};
use lazy_static::*;
use alloc::{sync::Arc, vec::Vec};
use crate::{memory::*};
use crate::sync::UPSafeCell;

const VIRTIO0: usize = 0x10001000;

lazy_static!{
    static ref QUEUE_FRAMES:UPSafeCell<Vec<(virtio_drivers::PhysAddr, Vec<FramTracker>)>> =
        UPSafeCell::new(Vec::new());
}





pub struct VirtBlk(pub UPSafeCell<VirtIOBlk<'static,VirtioHal>>, u64);



impl VirtBlk {
    pub fn new()->Self{
        unsafe {
            let header = &mut *(VIRTIO0 as *mut VirtIOHeader);
            let capacity_in_sectors = core::ptr::read_volatile(header.config_space() as *const u64);
            VirtBlk(
                UPSafeCell::new(
                    VirtIOBlk::new(header).expect("failed new blk device")
                ),
                capacity_in_sectors,
            )
        }
    }

    pub fn capacity_in_sectors(&self) -> u64 {
        self.1
    }
}

pub struct VirtioHal;
impl Hal for VirtioHal {
    fn dma_alloc(pages: usize) -> virtio_drivers::PhysAddr {
        let frames = alloc_contiguous_frames(pages).expect("no contiguous frames alloced");
        let base_ppn = frames
            .first()
            .map(|f| f.ppn)
            .unwrap_or(PhysiNumber(0));
        let base_addr: PhysiAddr = base_ppn.into();

        unsafe {
            let len = pages * crate::config::PAGE_SIZE;
            core::slice::from_raw_parts_mut(base_addr.0 as *mut u8, len).fill(0);
        }

        QUEUE_FRAMES.lock().push((base_addr.0, frames));
        base_addr.0
    }
    fn dma_dealloc(paddr: virtio_drivers::PhysAddr, pages: usize) -> i32 {
        let mut q = QUEUE_FRAMES.lock();
        if let Some(pos) = q.iter().position(|(base, v)| *base == paddr && v.len() == pages) {
            let (_, frames) = q.remove(pos);
            drop(frames);
            0
        } else {
            -1
        }
    }
    fn phys_to_virt(paddr: virtio_drivers::PhysAddr) -> virtio_drivers::VirtAddr {
        paddr
    }
    fn virt_to_phys(vaddr: virtio_drivers::VirtAddr) -> virtio_drivers::PhysAddr {
        // Buffers passed into virtio_drivers may live on kernel stacks/heaps which are
        // mapped by the kernel page table and are NOT guaranteed to be identity-mapped.
        // Translate via kernel page table to obtain a real physical address.
        let mut table = PageTable::get_kernel_table_layer();
        if let Some(paddr) = table.translate(VirAddr(vaddr)) {
            paddr.0
        } else {
            // Fallback to identity mapping for early-boot / direct-mapped regions.
            vaddr
        }
    }
}

