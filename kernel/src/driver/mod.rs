// DTB (Device Tree Blob) parser
pub mod dtb;
pub mod gpu;
pub mod network;
pub mod nvme;
pub mod pcie;
#[cfg(target_arch = "riscv64")]
pub mod usb;
