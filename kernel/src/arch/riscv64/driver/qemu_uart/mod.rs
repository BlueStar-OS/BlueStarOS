//! QEMU virt RISC-V 16550 UART driver.
//!
//! This module is compiled only for the `riscv64-qemu` board profile. The
//! QEMU virt UART uses byte-spaced registers at `0x1000_0000` and connects to
//! the SiFive PLIC. Device-tree metadata is still used to override the base
//! address when a compatible `ns16550a` node is supplied.

use crate::arch::driver;
use crate::driver::dtb::DeviceNode;
use crate::kprintln;

/// QEMU virt UART base address before DTB probing.
static mut UART0_BASE: usize = 0x1000_0000;

/// UART register indices for byte-spaced 16550 access.
const UART_THR_RBR: usize = 0;
const UART_IER: usize = 1;
const UART_IIR: usize = 2;
const UART_LSR: usize = 5;
const UART_MSR: usize = 6;

/// QEMU virt UART interrupt source in the SiFive PLIC.
pub const UART0_IRQ: u32 = 10;

/// Write one byte to the UART transmit holding register.
pub fn putc(c: u8) {
    // SAFETY: UART0_BASE is a board MMIO address selected before use and the
    // volatile write is the required side effect for a device register.
    unsafe {
        let uart = (UART0_BASE + UART_THR_RBR) as *mut u8;
        uart.write_volatile(c);
    }
}

/// Poll the UART receive buffer and return one byte when available.
pub fn getc() -> Option<u8> {
    // SAFETY: UART0_BASE points to the QEMU 16550 MMIO block; volatile reads
    // are used for both status and receive registers.
    unsafe {
        let lsr = (UART0_BASE + UART_LSR) as *const u8;
        let rbr = (UART0_BASE + UART_THR_RBR) as *const u8;
        if lsr.read_volatile() & 1 != 0 {
            Some(rbr.read_volatile())
        } else {
            None
        }
    }
}

/// Poll until a byte is received.
pub fn getc_blocking() -> u8 {
    loop {
        if let Some(c) = getc() {
            return c;
        }
        core::hint::spin_loop();
    }
}

/// Probe the QEMU UART node and register its MMIO mapping and PLIC handler.
fn uart_16550_probe(node: &DeviceNode, _compatible: &str) -> Result<(), &'static str> {
    let reg = node.get_property("reg").ok_or("Missing reg property")?;
    let regs = reg.as_reg(2, 2);
    let first = regs.first().ok_or("Empty reg property")?;
    let base_addr = first.address as usize;
    let size = first.size as usize;
    if size == 0 {
        return Err("UART MMIO size is zero");
    }

    if node.get_u32("reg-shift").unwrap_or(0) != 0 {
        return Err("QEMU UART reg-shift must be zero");
    }

    kprintln!(
        "[UART Probe] Found QEMU UART at {:#x}, size={:#x}",
        base_addr,
        size
    );

    // SAFETY: probe runs during single-hart platform initialization before
    // the UART is accessed concurrently.
    unsafe {
        UART0_BASE = base_addr;
    }

    {
        use crate::arch::memory::VirAddr;
        use crate::memory::{register_kernel_mmio, MapAreaFlags, VirNumRange};

        let mmio_range = VirNumRange::new(VirAddr(base_addr), VirAddr(base_addr + size - 1));
        let flags = MapAreaFlags::V
            | MapAreaFlags::R
            | MapAreaFlags::W
            | MapAreaFlags::A
            | MapAreaFlags::G
            | MapAreaFlags::DEV;
        register_kernel_mmio(mmio_range, flags);
    }

    driver::plic::register_irq(UART0_IRQ, driver::keyboard::keyboard_interrupt_handler, 1);
    Ok(())
}

crate::dtb_probe! {
    compatible: "ns16550a",
    priority: Mid,
    driver: "qemu-16550",
    probe: uart_16550_probe
}

/// QEMU keyboard support uses these register helpers from its interrupt path.
pub fn enable_rx_interrupt() {
    // SAFETY: UART0_BASE points to the initialized QEMU UART MMIO block.
    unsafe {
        ((UART0_BASE + UART_IER) as *mut u8).write_volatile(1);
    }
}

/// Read the UART interrupt identification register.
pub fn read_iir() -> u8 {
    // SAFETY: UART0_BASE points to the initialized QEMU UART MMIO block.
    unsafe { ((UART0_BASE + UART_IIR) as *const u8).read_volatile() }
}

/// Read the line-status register.
pub fn read_lsr() -> u8 {
    // SAFETY: UART0_BASE points to the initialized QEMU UART MMIO block.
    unsafe { ((UART0_BASE + UART_LSR) as *const u8).read_volatile() }
}

/// Read the modem-status register.
pub fn read_msr() -> u8 {
    // SAFETY: UART0_BASE points to the initialized QEMU UART MMIO block.
    unsafe { ((UART0_BASE + UART_MSR) as *const u8).read_volatile() }
}

/// The generic TTY-facing UART interface for this board.
pub mod uart {
    pub use super::{getc, getc_blocking, putc};
}
