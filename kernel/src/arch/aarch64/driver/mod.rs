//! 平台设备驱动探测和初始化
mod rk3588_uart; //香橙派5plus rk3588串口驱动
mod qemu_uart;
pub mod gicd;
pub mod keyboard;
pub mod emmc_blk;

// pub use self::rk3588_uart::uart;
pub use self::qemu_uart::uart;
