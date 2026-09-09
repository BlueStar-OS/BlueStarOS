//! 等待队列：用于信号量、条件变量、socket 收包等阻塞/唤醒机制。
//!
//! 维护自己的等待者列表。`block` 时将当前任务从 `task_queen` 移除
//! 并 clone 进 `waiters`，`wake` 时弹出一个任务并将其重新放回 `task_queen`。
//!
//! 任务在阻塞期间不占用 `task_queen` 槽位，仅通过 WaitQueue 持有引用。
//!
//! ## IRQ 安全性
//!
//! `NoIrqLock` 在借用期间关闭中断，因此 `wake()` 也可以由已经进入 PLIC
//! 分发路径的硬中断调用；它不会重入一个正在持有的调度器借用。

use alloc::{sync::Arc, vec::Vec};

use crate::{
    sync::NoIrqLock,
    task::{TaskControlBlock, TaskStatus, TASK_MANAER},
};

/// 等待队列。
///
/// `block()` 将当前任务设为 Blocking，从 `task_queen` 移除，推入 `waiters`，
/// 然后切换到下一个就绪任务。`wake()` 从 `waiters` 弹出一个任务，
/// 设为 Ready 并推回 `task_queen`。
///
/// 这里持有的是 TCB 的 `Arc`，不是任务栈或上下文本身的所有权；真正的上下文
/// 切换仍由 `TaskManager` 完成。
pub struct WaitQueue {
    waiters: NoIrqLock<Vec<Arc<NoIrqLock<TaskControlBlock>>>>,
}

impl WaitQueue {
    pub fn new() -> Self {
        Self {
            waiters: NoIrqLock::new(Vec::new()),
        }
    }

    /// 将当前任务加入此等待队列并阻塞。
    ///
    /// 1. 获取当前任务的 TCB clone，设为 Blocking，推入 waiters
    /// 2. 调用 TaskManager 从 task_queen 移除当前任务并切换到下一个就绪任务
    pub fn block(&self) {
        // 获取当前任务 TCB 并推入 waiters
        let tcb = TASK_MANAER
            .task_que_inner
            .lock(|inner| inner.task_queen[inner.current].clone());
        tcb.lock(|t| t.task_statut = TaskStatus::Blocking);
        self.waiters.lock(|w| w.push(tcb));

        // 从 task_queen 移除当前任务，stride 调度选下一个，执行 __switch
        TASK_MANAER.block_and_switch();
    }

    /// 从等待队列唤醒一个任务。
    ///
    /// 1. 从 waiters 弹出一个任务
    /// 2. 设为 Ready，推回 task_queen
    pub fn wake(&self) {
        let tcb = self.waiters.lock(|w| w.pop());
        if let Some(tcb) = tcb {
            tcb.lock(|t| t.task_statut = TaskStatus::Ready);
            TASK_MANAER.task_que_inner.lock(|inner| {
                inner.task_queen.push_back(tcb);
            });
        }
    }
}
