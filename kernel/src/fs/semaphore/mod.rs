//! 阻塞与唤醒原语。
//!
//! 当前模块服务于 VFS、socket 收包和后续事件源：
//! - `waitqueue`: 直接把当前任务挂入等待队列并切换调度；
//! - `eventsource`: 预留“IRQ 先记录事件，任务上下文再唤醒”的 bottom-half 入口。
//!
//! 注意：`UPSafeCell` 不是可重入锁。任何可能从 IRQ 调用的唤醒路径，都要避免
//! 与普通任务上下文同时借用调度器内部队列。

pub mod eventsource;
pub mod waitqueue;
