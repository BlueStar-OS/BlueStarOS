// 架构抽象层统一导出

#[cfg(target_arch = "riscv64")]
pub mod riscv64;

// 统一导出架构相关函数
#[cfg(target_arch = "riscv64")]
pub use riscv64::*;

#[cfg(target_arch = "aarch64")]
pub mod aarch64;

#[cfg(target_arch = "aarch64")]
pub use aarch64::*;
