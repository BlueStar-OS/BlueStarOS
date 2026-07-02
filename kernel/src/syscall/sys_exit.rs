//! sys_exit — 终止当前任务并进入 Zombie。
//!
//! ## 作用
//! 终止当前任务并进入 Zombie。
//!
//! ## 参数
//! `exit_code` 退出码。
//!
//! ## 注意事项
//! 保留 Zombie 等待父进程 wait 回收；init 退出触发关机。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: kernel/exit.c:1072
//!
//! ## 实现情况
//! 已实现基础路径。

use crate::shutdown;
use crate::syscall::syscall::*;
use crate::task::INIT_PID;

pub fn sys_exit(exit_code: usize) -> isize {
    // 若把 init 标记为 Zombie，会导致系统只剩 Zombie/无 Ready 任务，从而调度器报错。
    let (_current_pid, current_task) = TASK_MANAER.task_que_inner.lock(|inner| {
        if inner.task_queen.is_empty() {
            (0, None)
        } else {
            let current = inner.current;
            let pid = inner.task_queen[current].lock(|t| t.pid.0);
            let ts = inner.task_queen[current].clone();
            (pid, Some(ts))
        }
    });

    if let Some(ts) = current_task {
        // init退出
        ts.lock(|lock| {
            if lock.pid.0 == INIT_PID {
                error!("INIT PROC EXIT!");
                shutdown();
            }
        });

        // 提取 parent 引用，mark_zombie 之后再唤醒
        let parent_weak = ts.lock(|t| t.parent.clone());

        // Linux 语义：exit 后任务进入 Zombie，保留 pid/exit_code，等待父进程 wait() 回收(reap)。
        // 父进程退出时，其子进程会被过继给 init(pid=1)。
        if exit_code == 0 {
            //warn!("Program Exit Normaly With Code:{}", exit_code);
        } else {
            warn!("Program Exit With Code:{}", exit_code);
        }
        TASK_MANAER.reparent_current_children_to_init();
        TASK_MANAER.mark_current_zombie(exit_code as isize);

        // 通过 exit_queue 唤醒父进程 (必须在 mark_zombie 之后，父进程醒来后检查 children 必须看到 Zombie 状态)。
        // 这里必须先在父 TCB 锁内 clone 出 `exit_queue`，然后在锁外执行 wake()；
        // 否则父进程若正阻塞在自己的 exit_queue 上，wake() 会再次借用同一个 TCB，
        // 触发 UPSafeCell double borrow。
        // 参考: kernel/exit.c:645-691,1413-1415
        let parent_exit_queue = parent_weak
            .and_then(|pa_weak| pa_weak.upgrade())
            .map(|pa| pa.lock(|p| p.exit_queue.clone()));
        if let Some(exit_queue) = parent_exit_queue {
            exit_queue.wake();
        }

        // 进入 Zombie 后必须立刻让出 CPU
        TASK_MANAER.suspend_and_run_task();
    }
    BlueErr::ENOSYS.as_isize()
}
