//! 平台设备驱动探测和初始化
pub mod emmc_blk;
pub mod gicd;
pub mod keyboard;
mod qemu_uart;
mod rk3588_uart; //香橙派5plus rk3588串口驱动
pub mod virtio_blk;

// pub use self::rk3588_uart::uart;
pub use self::qemu_uart::irq_intid as uart_irq_intid;
pub use self::qemu_uart::uart;
