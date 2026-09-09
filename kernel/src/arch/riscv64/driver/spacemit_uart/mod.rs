//! SpacemiT K3 COM260 Kit UART driver.
//!
//! The board DTB describes `serial0` at `0xd4017000` with
//! `compatible = "spacemit,k1-uart", "intel,xscale-uart"`,
//! `reg-io-width = <4>`, and `reg-shift = <2>`:
//! `/home/inkbottle/othersrc/k3/k3.dts:6168-6182`.
//! The K3 APLIC/IMSIC interrupt path is not implemented yet, so input uses
//! polling and this driver never writes the QEMU PLIC register layout.

use crate::driver::dtb::DeviceNode;
use crate::kprintln;

/// K3 UART base address before the DTB probe runs.
static mut UART0_BASE: usize = 0xd401_7000;

/// K1 UART register indices before applying `reg-shift = <2>`.
const UART_REG_SHIFT: usize = 2;
const UART_THR_RBR: usize = 0;
const UART_LSR: usize = 5;

/// Write one 32-bit register access to the K3 UART transmit register.
pub fn putc(c: u8) {
    // SAFETY: UART0_BASE is the K3 UART MMIO address selected by the board
    // profile or validated by the DTB probe; K3 requires 32-bit accesses.
    unsafe {
        let uart = (UART0_BASE + (UART_THR_RBR << UART_REG_SHIFT)) as *mut u32;
        uart.write_volatile(c as u32);
    }
}

/// Poll the K3 UART line-status register and read one byte when available.
pub fn getc() -> Option<u8> {
    // SAFETY: UART0_BASE points to the K3 UART MMIO block and both accesses
    // are volatile device-register operations.
    unsafe {
        let lsr = (UART0_BASE + (UART_LSR << UART_REG_SHIFT)) as *const u32;
        let rbr = (UART0_BASE + (UART_THR_RBR << UART_REG_SHIFT)) as *const u32;
        if lsr.read_volatile() & 1 != 0 {
            Some(rbr.read_volatile() as u8)
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

/// Probe the K3 UART and register its MMIO range for the post-MMU kernel.
fn k1_uart_probe(node: &DeviceNode, _compatible: &str) -> Result<(), &'static str> {
    let reg = node.get_property("reg").ok_or("Missing reg property")?;
    let regs = reg.as_reg(2, 2);
    let first = regs.first().ok_or("Empty reg property")?;
    let base_addr = first.address as usize;
    let size = first.size as usize;
    if size == 0 {
        return Err("K3 UART MMIO size is zero");
    }
    if node.get_u32("reg-shift").unwrap_or(0) as usize != UART_REG_SHIFT {
        return Err("K3 UART reg-shift must be two");
    }
    if node.get_u32("reg-io-width").unwrap_or(4) != 4 {
        return Err("K3 UART reg-io-width must be four");
    }

    kprintln!(
        "[UART Probe] Found K3 UART at {:#x}, size={:#x}",
        base_addr,
        size
    );

    // SAFETY: probe runs during single-hart platform initialization before
    // the UART is accessed concurrently.
    unsafe {
        UART0_BASE = base_addr;
    }

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
    Ok(())
}

crate::dtb_probe! {
    compatible: "spacemit,k1-uart",
    priority: Mid,
    driver: "spacemit-k1-uart",
    probe: k1_uart_probe
}

/// The generic TTY-facing UART interface for this board.
pub mod uart {
    pub use super::{getc, getc_blocking, putc};
}
