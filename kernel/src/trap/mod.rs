use crate::arch::memory::*;
use crate::memory::VirNumRange;
use crate::sync::UPSafeCell;
use crate::syscall::*; //系统调用
use crate::MapSet;
use crate::{
    config::*, shutdown, task::TASK_MANAER, time::set_next_timeInterupt,
    trap::pagefaultHandler::PageFaultHandler,
};
use alloc::vec::Vec;
use core::arch::asm;
use core::{arch::global_asm, panic};
use lazy_static::lazy_static;
use log::warn;
use log::{debug, error};
use riscv::register::satp;
use riscv::register::scause::Interrupt;
use riscv::register::sie;
use riscv::register::{
    scause::{self, Exception, Trap},
    sie::Sie,
    sscratch,
    sstatus::{self, Sstatus, SPP},
    stval, stvec,
    utvec::TrapMode,
};

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
