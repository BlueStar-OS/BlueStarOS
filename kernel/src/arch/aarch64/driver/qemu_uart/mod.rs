// QEMU AArch64 PL011 UART driver.
static mut UART0_BASE: usize = 0x0900_0000;
static mut UART0_INTID: u32 = 33;

const UART_DR: usize = 0x00;
const UART_FR: usize = 0x18;
const UART_IMSC: usize = 0x38;
const UART_MIS: usize = 0x40;
const UART_ICR: usize = 0x44;

const FR_RXFE: u32 = 1 << 4;
const FR_TXFF: u32 = 1 << 5;
const IMSC_RXIM: u32 = 1 << 4;
const IMSC_RTIM: u32 = 1 << 6;
const ICR_RXIC: u32 = 1 << 4;
const ICR_RTIC: u32 = 1 << 6;

use crate::kprintln;

#[inline]
fn read32(offset: usize) -> u32 {
    unsafe { ((UART0_BASE + offset) as *const u32).read_volatile() }
}

#[inline]
fn write32(offset: usize, value: u32) {
    unsafe {
        ((UART0_BASE + offset) as *mut u32).write_volatile(value);
    }
}

#[inline]
pub fn base_addr() -> usize {
    unsafe { UART0_BASE }
}

#[inline]
pub fn irq_intid() -> u32 {
    unsafe { UART0_INTID }
}

#[inline]
pub fn read_fr() -> u32 {
    read32(UART_FR)
}

#[inline]
pub fn read_imsc() -> u32 {
    read32(UART_IMSC)
}

#[inline]
pub fn read_mis() -> u32 {
    read32(UART_MIS)
}

#[inline]
pub fn clear_rx_interrupts() {
    write32(UART_ICR, ICR_RXIC | ICR_RTIC);
}

pub fn enable_rx_interrupt() {
    let current = read_imsc();
    write32(UART_IMSC, current | IMSC_RXIM | IMSC_RTIM);
}

#[inline]
pub fn putc(c: u8) {
    while read_fr() & FR_TXFF != 0 {
        core::hint::spin_loop();
    }
    write32(UART_DR, c as u32);
}

#[inline]
pub fn getc() -> Option<u8> {
    if read_fr() & FR_RXFE != 0 {
        None
    } else {
        Some((read32(UART_DR) & 0xff) as u8)
    }
}

pub fn getc_blocking() -> u8 {
    loop {
        if let Some(c) = getc() {
            return c;
        }
        core::hint::spin_loop();
    }
}


use crate::driver::dtb::DeviceNode;
use log::{info, warn};

fn gic_intid_from_dtb_irq(cells: &[u32]) -> Option<u32> {
    if cells.len() < 2 {
        return None;
    }
    let irq_type = cells[0];
    let irq_num = cells[1];
    match irq_type {
        0 => Some(irq_num + 32), // SPI
        1 => Some(irq_num + 16), // PPI
        _ => Some(irq_num),
    }
}

fn arm_pl011_probe(node: &DeviceNode, _compatible: &str) -> Result<(), &'static str> {
    let reg = node.get_property("reg").ok_or("Missing reg property")?;
    let regs = reg.as_reg(2, 2);

    if regs.is_empty() {
        return Err("Empty reg property");
    }

    let base_addr = regs[0].address as usize;
    kprintln!(
        "[UART Probe] Found UART at {:#x}, size={:#x}",
        base_addr,
        regs[0].size
    );

    unsafe {
        UART0_BASE = base_addr;
    }

    if let Some(interrupts) = node.get_property("interrupts") {
        let cells = interrupts.as_u32_list();
        if let Some(intid) = gic_intid_from_dtb_irq(&cells) {
            unsafe {
                UART0_INTID = intid;
            }
            info!(
                "[UART Probe] PL011 IRQ cells={:?}, resolved INTID={}",
                cells, intid
            );
        } else {
            warn!("[UART Probe] Failed to decode interrupts property: {:?}", cells);
        }
    } else {
        warn!("[UART Probe] Missing interrupts property, fallback INTID={}", irq_intid());
    }

    {
        use crate::arch::memory::VirAddr;
        use crate::memory::{register_kernel_mmio, MapAreaFlags, VirNumRange};

        let mmio_range = VirNumRange::new(
            VirAddr(base_addr),
            VirAddr(base_addr + (regs[0].size as usize) - 1),
        );
        let flags = MapAreaFlags::V
            | MapAreaFlags::R
            | MapAreaFlags::W
            | MapAreaFlags::A
            | MapAreaFlags::G
            | MapAreaFlags::DEV;
        register_kernel_mmio(mmio_range, flags);
    }

    Ok(())
}

crate::dtb_probe! {
    compatible: "arm,pl011",
    priority: Mid,
    driver: "armpl011",
    probe: arm_pl011_probe
}

pub mod uart {
    pub use super::{getc, getc_blocking, putc};
}
