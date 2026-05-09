use crate::memory::VirNumRange;
use crate::sync::UPSafeCell;
//系统调用
use crate::config::*;
use crate::MapSet;
use alloc::vec::Vec;
use lazy_static::lazy_static;

pub mod pagefaultHandler;

lazy_static! {
    static ref PENDING_KSTACK_FREE: UPSafeCell<Vec<(VirNumRange, Option<usize>)>> =
        unsafe { UPSafeCell::new(Vec::new()) };
}

pub fn enqueue_kstack_free(range: VirNumRange, id0: Option<usize>) {
    let mut q = PENDING_KSTACK_FREE.lock();
    q.push((range, id0));
    drop(q);
}

pub fn recycle_pending_kstacks() {
    let mut pending = PENDING_KSTACK_FREE.lock();
    if pending.is_empty() {
        return;
    }

    let mut kspace = KERNEL_SPACE.lock();
    while let Some((range, id0)) = pending.pop() {
        while let Some(mut area) = kspace.pop_one_contain_range_area(range) {
            let mut vpn = area.range.0;
            while vpn.0 <= area.range.1 .0 {
                if area.frames.contains_key(&vpn) {
                    area.unmap_one(&mut kspace.table, vpn);
                } else {
                    kspace.table.unmap(vpn);
                }
                vpn.step();
            }
        }
        if let Some(id0) = id0 {
            MapSet::dealloc_kernel_stack_id0(id0);
        }
    }
}
