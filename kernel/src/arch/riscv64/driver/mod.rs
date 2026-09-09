//! RISC-V board driver selection and common interrupt dispatch.

pub mod dma;
pub mod keyboard;

#[cfg(feature = "riscv64-qemu")]
pub mod plic;
#[cfg(feature = "riscv64-qemu")]
pub mod qemu_uart;
#[cfg(feature = "spacemitk3-com260kit")]
pub mod spacemit_uart;
#[cfg(feature = "riscv64-qemu")]
pub mod virtio_blk;

#[cfg(feature = "riscv64-qemu")]
pub use self::qemu_uart::uart;
#[cfg(feature = "spacemitk3-com260kit")]
pub use self::spacemit_uart::uart;

/// Initialize the external-interrupt controller selected by the board.
pub fn init_external_interrupts() {
    #[cfg(feature = "riscv64-qemu")]
    {
        plic::plic_init();
        keyboard::enable_uart_rx_interrupt();
        crate::arch::riscv64::enable_external_interrupt();
    }

    #[cfg(feature = "spacemitk3-com260kit")]
    crate::kprintln!("[ArchInit] K3 APLIC/IMSIC IRQ path is deferred; UART input uses polling");
}

/// Dispatch one supervisor-external interrupt for the selected board.
pub fn dispatch_external_interrupt() {
    #[cfg(feature = "riscv64-qemu")]
    plic::dispatch_irq();

    #[cfg(feature = "spacemitk3-com260kit")]
    {
        // TODO(k3-aplic): implement APLIC source claim/complete and IMSIC
        // delivery before enabling supervisor external interrupts on K3.
    }
}
