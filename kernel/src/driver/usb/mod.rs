//! USB host-controller drivers.

#[cfg(target_arch = "riscv64")]
#[path = "usb-host/xhci_host/xhci_host.rs"]
pub mod xhci_host;

#[cfg(target_arch = "riscv64")]
#[path = "usb-host/qemu_xhcihost.rs"]
pub mod qemu_xhcihost;
