//! e1000 硬件相关模块。
//!
//! 负责设备状态、寄存器定义、probe 初始化、中断控制。

pub mod device;
pub mod irq;
pub mod probe;
pub mod regs;
