// 信号优先级包装
// 将 bitflags Signal 包装为带优先级的 OsSignal，按优先级排序投递

use super::Signal;
use alloc::collections::vec_deque::VecDeque;

/// 带优先级的信号
#[derive(Clone, Debug)]
pub struct OsSignal {
    pub signal: Signal,
    /// 数值越小优先级越高: 0=不可屏蔽, 1=终止类, 2=普通
    pub priority: u8,
}

impl OsSignal {
    pub fn new(signal: Signal) -> Self {
        let priority = signal_priority(signal);
        Self { signal, priority }
    }
}

/// 根据 Linux 信号语义分配优先级
fn signal_priority(sig: Signal) -> u8 {
    if sig.contains(Signal::SIGKILL) || sig.contains(Signal::SIGSTOP) {
        return 0;
    }
    if sig.contains(Signal::SIGINT)
        || sig.contains(Signal::SIGQUIT)
        || sig.contains(Signal::SIGTERM)
        || sig.contains(Signal::SIGABRT)
    {
        return 1;
    }
    2
}

/// 按优先级插入信号（优先级高的排前面）
pub fn push_signal(queue: &mut VecDeque<OsSignal>, sig: OsSignal) {
    let pos = queue
        .iter()
        .position(|s| s.priority > sig.priority)
        .unwrap_or(queue.len());
    queue.insert(pos, sig);
}
