//! RISC-V UART keyboard input buffer.
//!
//! The buffer is shared by QEMU's interrupt path and K3's polling path. Board
//! specific register access remains in the selected `uart` driver module.

use crate::arch::driver::uart;
use crate::sync::NoIrqLock;
use alloc::collections::vec_deque::VecDeque;
use lazy_static::lazy_static;

#[cfg(feature = "riscv64-qemu")]
use crate::task::signal::{push_signal, OsSignal};
#[cfg(feature = "riscv64-qemu")]
use crate::task::{Signal, TASK_MANAER};

lazy_static! {
    /// Buffered characters produced by the QEMU interrupt path.
    static ref INPUT_BUF: NoIrqLock<VecDeque<u8>> = NoIrqLock::new(VecDeque::with_capacity(256));
}

/// Enable QEMU's 16550 receive interrupt.
#[cfg(feature = "riscv64-qemu")]
pub fn enable_uart_rx_interrupt() {
    crate::arch::driver::qemu_uart::enable_rx_interrupt();
    crate::kprintln!("[Keyboard] QEMU UART RX interrupt enabled");
}

/// Drain all pending QEMU UART interrupt causes and buffer received bytes.
#[cfg(feature = "riscv64-qemu")]
pub fn keyboard_interrupt_handler() {
    loop {
        let iir = crate::arch::driver::qemu_uart::read_iir();
        // IIR bit 0 = no interrupt pending.
        if iir & 1 != 0 {
            break;
        }
        match (iir >> 1) & 0x07 {
            // RX data available or character timeout.
            0b010 | 0b110 => {
                while let Some(c) = uart::getc() {
                    handle_char(c);
                }
            }
            // TX empty was acknowledged by reading IIR.
            0b001 => {}
            // Line status or modem status: read the corresponding register.
            0b011 => {
                let _ = crate::arch::driver::qemu_uart::read_lsr();
            }
            0b000 => {
                let _ = crate::arch::driver::qemu_uart::read_msr();
            }
            _ => break,
        }
    }
}

#[cfg(feature = "riscv64-qemu")]
fn handle_char(c: u8) {
    match c {
        0x03 => push_signal_to_current(Signal::SIGINT),
        0x1C => push_signal_to_current(Signal::SIGQUIT),
        0x1A => push_signal_to_current(Signal::SIGTSTP),
        _ => INPUT_BUF.try_lock(|lock| {
            if let Some(buf) = lock {
                buf.push_back(c);
            }
        }),
    }
}

/// Return one buffered character, or poll the selected board UART directly.
pub fn read_input() -> Option<u8> {
    let buffered = INPUT_BUF.lock(|buf| buf.pop_front());
    buffered.or_else(uart::getc)
}

#[cfg(feature = "riscv64-qemu")]
fn push_signal_to_current(sig: Signal) {
    TASK_MANAER.task_que_inner.try_lock(|inner_opt| {
        let Some(inner) = inner_opt else { return };
        if inner.task_queen.is_empty() {
            return;
        }
        let current = inner.current;
        if current >= inner.task_queen.len() {
            return;
        }
        let current_task = inner.task_queen[current].clone();
        current_task.try_lock(|task_opt| {
            if let Some(task) = task_opt {
                push_signal(&mut task.signal, OsSignal::new(sig));
            }
        });
    });
}
