//! AArch64 keyboard input via QEMU PL011 RX interrupt.

use crate::sync::UPSafeCell;
use alloc::collections::VecDeque;
use lazy_static::lazy_static;
use log::{debug, info, warn};

lazy_static! {
    static ref INPUT_BUF: UPSafeCell<VecDeque<u8>> =
        unsafe { UPSafeCell::new(VecDeque::with_capacity(128)) };
}

pub fn enable_uart_rx_interrupt() {
    use super::gicd::gic_enable_spi;
    use super::qemu_uart;

    qemu_uart::enable_rx_interrupt();
    qemu_uart::clear_rx_interrupts();

    let irq = qemu_uart::irq_intid();
    gic_enable_spi(irq);

    let imsc = qemu_uart::read_imsc();
    let mis = qemu_uart::read_mis();
    let fr = qemu_uart::read_fr();

    crate::kprintln!(
        "[Keyboard] PL011 RX interrupt enabled: base={:#x} intid={} IMSC={:#x} MIS={:#x} FR={:#x}",
        qemu_uart::base_addr(),
        irq,
        imsc,
        mis,
        fr
    );
}

pub fn keyboard_interrupt_handler() {
    use super::qemu_uart;

    let mis = qemu_uart::read_mis();
    let fr = qemu_uart::read_fr();
    debug!("[Keyboard] IRQ entry: MIS={:#x} FR={:#x}", mis, fr);

    let mut drained = 0usize;
    while let Some(c) = crate::arch::driver::uart::getc() {
        drained += 1;
        debug!("[Keyboard] RX char={:#x} '{}'", c, c as char);
        handle_char(c);
    }
    qemu_uart::clear_rx_interrupts();
    debug!(
        "[Keyboard] IRQ exit: drained={} MIS={:#x} FR={:#x}",
        drained,
        qemu_uart::read_mis(),
        qemu_uart::read_fr()
    );
}

fn handle_char(c: u8) {
    match c {
        0x03 => {}
        0x1C => {}
        0x1A => {}
        _ => {
            info!("[Keyboard] buffer char={:#x} '{}'", c, c as char);
            INPUT_BUF.lock().push_back(c);
        }
    }
}

pub fn read_input() -> Option<u8> {
    INPUT_BUF.lock().pop_front()
}
