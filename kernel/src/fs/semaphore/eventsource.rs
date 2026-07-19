//! 事件源占位模块。
//!
//! `WaitQueue::wake()` 目前会直接操作调度队列；当调用方来自 IRQ 路径时，
//! 这会和普通任务上下文争用 `TASK_MANAER.task_que_inner` 的 `UPSafeCell` 借用。
//! 后续网络收包应优先演进为：
//!
//! ```text
//! hard IRQ:   只确认设备中断、记录 pending event、屏蔽或重新使能中断
//! softirq:    轮询 RX ring、分发协议包、调用 WaitQueue::wake()
//! task ctx:   recvfrom/read 阻塞在 WaitQueue::block()
//! ```
//!
//! 这个模块先保留轻量类型，作为后续 softirq / event loop 的落点，避免把
//! 延迟唤醒状态散落到 e1000 或 UDP socket 内。

/// 延迟唤醒事件源。
///
/// 当前仅作为设计占位，不持有状态。等引入 softirq/bottom-half 后，可以在
/// 这里增加 pending 位图或队列，并提供从 IRQ 安全路径提交事件的接口。
pub struct EventSource;

impl EventSource {
    /// 创建空事件源。
    pub const fn new() -> Self {
        Self
    }
}
