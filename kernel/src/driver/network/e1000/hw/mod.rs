//! e1000 硬件相关模块。
//!
//! 负责设备状态、寄存器定义、probe 初始化、中断控制。
//! 这里不解析协议包，也不直接暴露 socket 语义；IRQ 只把完成的 RX 帧交给
//! `packet::network_packet_resolve`，TX 完成则回收描述符资源。

pub mod device;
pub mod irq;
pub mod probe;
pub mod regs;
