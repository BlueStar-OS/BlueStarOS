// VirtIO drivers - only for RISC-V QEMU
#[cfg(target_arch = "riscv64")]
mod virtio_blk;

// Re-exports
#[cfg(target_arch = "riscv64")]
pub use self::virtio_blk::*;



// UART drivers
// #[cfg(target_arch = "riscv64")]
mod qemu_uart;
pub mod uart {
    pub use super::qemu_uart::{putc, getc, getc_blocking};
}







// #[cfg(target_arch = "aarch64")]
// mod rk3588_uart;
// #[cfg(target_arch = "aarch64")]
// pub mod uart {
//     use super::rk3588_uart::*;
    
//     pub fn putc(c: u8) {
//         UART.putc(c);
//     }

//     pub fn getc() -> Option<u8> {
//         UART.getc()
//     }

//     pub fn getc_blocking() -> u8 {
//         UART.getc_blocking()
//     }
// }