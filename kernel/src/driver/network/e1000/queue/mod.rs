//! e1000 RX/TX 描述符环模块。
//!
//! `rx` / `tx` 只管理描述符环、DMA 缓冲区和硬件 doorbell：
//! - setup/configure：分配 ring 并写入设备寄存器；
//! - poll/clean/transmit：消费或提交描述符；
//! - free：释放 ring 持有的物理页所有权。
//!
//! 协议解析属于 `packet`，socket 唤醒属于 `network`/`fs::semaphore`。这里的
//! 注释重点保留 DMA 生命周期和内存屏障，避免把业务语义写进驱动队列层。

#[path = "../rx_ringbuffer.rs"]
pub mod rx;

#[path = "../tx_ringbuffer.rs"]
pub mod tx;
