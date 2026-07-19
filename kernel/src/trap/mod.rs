use crate::memory::VirNumRange;
use crate::sync::UPSafeCell;
//系统调用
use crate::config::*;
use crate::MapSet;
use alloc::vec::Vec;
use lazy_static::lazy_static;

pub mod pagefault_handler;

lazy_static! {
    static ref PENDING_KSTACK_FREE: UPSafeCell<Vec<(VirNumRange, Option<usize>)>> =
        UPSafeCell::new(Vec::new());
}

pub fn enqueue_kstack_free(range: VirNumRange, id0: Option<usize>) {
    PENDING_KSTACK_FREE.lock(|q| q.push((range, id0)));
}

pub fn recycle_pending_kstacks() {
    // 先取出所有 pending 项，释放 PENDING 锁后再操作 KERNEL_SPACE
    let items: Vec<_> = PENDING_KSTACK_FREE.lock(|pending| {
        if pending.is_empty() {
            return Vec::new();
        }
        core::mem::take(&mut *pending)
    });
    if items.is_empty() {
        return;
    }

    KERNEL_SPACE.lock(|kspace| {
        for (range, id0) in items {
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
    });
}
