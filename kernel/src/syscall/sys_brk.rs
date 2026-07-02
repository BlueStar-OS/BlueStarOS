//! sys_brk — 查询或调整用户堆顶。
//!
//! ## 作用
//! 查询或调整用户堆顶。
//!
//! ## 参数
//! `new_brk` 新堆顶虚拟地址，0 表示查询。
//!
//! ## 注意事项
//! 仅支持线性扩展/收缩当前进程 heap 区域。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: mm/mmap.c:115
//!
//! ## 实现情况
//! 已实现基础路径。

use crate::memory::VirNumRange;
use crate::syscall::syscall::*;

pub fn sys_brk(new_brk: VirAddr) -> isize {
    let new_brkaddr = new_brk.0;

    let satp = TASK_MANAER.get_current_stap();
    let mut tb = PageTable::crate_table_from_satp(satp);

    let current_task = TASK_MANAER
        .task_que_inner
        .lock(|inner| inner.task_queen[inner.current].clone());

    current_task.lock(|tcb| {
        let old_brk = tcb.memory_set.brk.0;

        if new_brkaddr == 0 {
            debug!("sys_brk: query, old_brk={:#x}", old_brk);
            return old_brk as isize;
        }

        if new_brkaddr <= old_brk {
            tcb.memory_set.brk = VirAddr(new_brkaddr);
            let recycle_start_addr: VirAddr = VirAddr(new_brkaddr).floor_up().into();
            if recycle_start_addr.0 < old_brk {
                tcb.memory_set
                    .unmap_range(recycle_start_addr, old_brk - recycle_start_addr.0);
            }
            debug!(
                "sys_brk: shrink new={:#x} old={:#x} -> ret={:#x}",
                new_brkaddr, old_brk, new_brkaddr
            );
            return new_brkaddr as isize;
        }

        let start_vpn: VirNumber = VirAddr(old_brk).floor_up();
        let end_vpn: VirNumber = VirAddr(new_brkaddr.saturating_sub(1)).floor_down();
        debug!(
            "sys_brk: start_vpn = {:#x} end_vpn={:#x}",
            start_vpn.0, end_vpn.0
        );

        if (start_vpn.0 <= 0x602 && end_vpn.0 >= 0x602)
            || (start_vpn.0 <= 0x603 && end_vpn.0 >= 0x603)
            || (start_vpn.0 <= 0x604 && end_vpn.0 >= 0x604)
        {
            warn!(
                "!!! BRK_EXPAND covers target vpns: start=0x{:x} end=0x{:x}",
                start_vpn.0, end_vpn.0
            );
        }

        if tcb
            .memory_set
            .AallArea_Iscontain_thisVpn_plus(VirNumRange(start_vpn, end_vpn))
        {
            return old_brk as isize;
        }

        if start_vpn.0 <= end_vpn.0 {
            tcb.memory_set.add_area(
                crate::memory::VirNumRange(start_vpn, end_vpn),
                crate::memory::MapType::Maped,
                crate::memory::MapAreaFlags::R
                    | crate::memory::MapAreaFlags::W
                    | crate::memory::MapAreaFlags::U,
                None,
                None,
            );
        }

        tcb.memory_set.brk = VirAddr(new_brkaddr);
        debug!(
            "sys_brk: expand new={:#x} old={:#x} start_vpn={:#x} end_vpn={:#x} -> ret={:#x}",
            new_brkaddr, old_brk, start_vpn.0, end_vpn.0, new_brkaddr
        );
        new_brkaddr as isize
    })
}
