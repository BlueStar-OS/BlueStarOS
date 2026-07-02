//! sys_fork — 复制当前任务生成子进程。
//!
//! ## 作用
//! 复制当前任务生成子进程。
//!
//! ## 参数
//! `mode` clone flags；`stack/ptid/tls/ctid` clone 参数。
//!
//! ## 注意事项
//! 使用深拷贝地址空间；多线程 clone 共享语义未实现。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: kernel/fork.c:2718-2734 (clone/fork 族)
//!
//! ## 实现情况
//! 已实现 fork 语义。

use crate::arch::task::TaskContext;
use crate::arch::TrapContext;
use crate::config::PAGE_SIZE;
use crate::memory::{CloneFlags, MapSet};
use crate::sync::UPSafeCell;
use crate::syscall::syscall::*;
use crate::task::{ProcessId_ALLOCTOR, TaskStatus};
use crate::TRAP_CONTEXT_ADDR;
use alloc::sync::Arc;

pub fn sys_fork(_mode: CloneFlags, stack: usize, _ptid: usize, _tls: usize, _ctid: usize) -> isize {
    // 先从父进程深拷贝一份新的地址空间
    let (current_task, parent_pid, mut bad_task, new_memset) =
        TASK_MANAER.task_que_inner.lock(|inner| {
            let current_index = inner.current;
            let current_task = inner.task_queen[current_index].clone();

            let new_memset = current_task.lock(|parent| parent.memory_set.clone_mapset());
            let parent_pid = current_task.lock(|t| t.pid.0);
            let bad_task = current_task.lock(|t| t.clone());

            (current_task, parent_pid, bad_task, new_memset)
        });

    bad_task.parent = None;
    bad_task.childrens.clear();

    let new_pid = ProcessId_ALLOCTOR
        .lock(|alloc| alloc.alloc_id())
        .expect("No Process ID Can use");
    // 不要让旧的 ProcessId(parent_pid) drop 回收 parent_pid，否则会污染 pid 池。
    let old_pid = core::mem::replace(&mut bad_task.pid, new_pid);
    core::mem::forget(old_pid);
    let child_pid = bad_task.pid.0;
    debug!("Parent:pid {} child:{}", parent_pid, child_pid);
    let shallow = core::mem::replace(&mut bad_task.memory_set, MapSet::new_bare());
    core::mem::forget(shallow);
    if new_memset.is_none() {
        error!("Process Memset clone failed!");
        return BlueErr::ENOMEM.as_isize();
    }
    bad_task.memory_set = new_memset.expect("Memset should be some");

    // 为子进程分配独立的内核栈，并同步到 TaskContext/TrapContext
    let child_kernel_sp = MapSet::alloc_kernel_stack();
    // 子进程第一次被调度必须从 app_entry_point 起步，才能通过 __restore 使用 TrapContext 恢复用户态寄存器。
    // 只修改 sp 会让子进程继承父进程的内核执行流，导致 fork 返回值等寄存器语义错误。
    bad_task.task_context = TaskContext::return_trap_new(child_kernel_sp);

    // child内核栈释放信息
    let child_kernel_range = MapSet::get_kernel_range_from_kernel_top(VirAddr(child_kernel_sp));
    bad_task.memory_set.kernel_stack_range = Some(child_kernel_range);

    bad_task.task_statut = TaskStatus::Ready; //设置任务准备被调度
    {
        let trap_cx_ppn = bad_task
            .memory_set
            .table
            .translate_byvpn(VirAddr(TRAP_CONTEXT_ADDR).strict_into_virnum())
            .expect("trap ppn translate failed");
        bad_task.trap_context_ppn = trap_cx_ppn.0;
        let trap_cx_point: *mut TrapContext = (trap_cx_ppn.0 * PAGE_SIZE) as *mut TrapContext;
        unsafe {
            (*trap_cx_point).kernel_sp = child_kernel_sp;
            (*trap_cx_point).x[10] = 0;

            // TODO THREAD define
            // stack
            if stack != 0 {
                (*trap_cx_point).x[2] = stack;
            }

            debug!(
                "fork child init: pid={} trap_ppn={} child_a0={}",
                child_pid,
                trap_cx_ppn.0,
                (*trap_cx_point).x[10]
            );
        }
    }

    let arc_task = Arc::new(UPSafeCell::new(bad_task));
    /* 建立父子关系 */
    current_task.lock(|parent| parent.add_children(arc_task.clone()));
    arc_task.lock(|child| child.set_father(&current_task));

    /* 把克隆后的任务添加到任务队列 */
    TASK_MANAER.task_que_inner.lock(|inner| {
        inner.task_queen.push_back(arc_task.clone());
    });

    child_pid as isize
}
