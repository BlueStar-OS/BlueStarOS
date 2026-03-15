//! AArch64 键盘中断处理（UART RX 中断驱动输入）
//!
//! 复用 RISC-V keyboard handler 的模式：
//! - 中断 handler 把字符存入缓冲区
//! - TTY 的 get_char 从缓冲区读取
//! - Ctrl+C/Z/\ 转换为进程信号

use alloc::collections::VecDeque;
use crate::sync::UPSafeCell;
use lazy_static::lazy_static;
use log::warn;

lazy_static! {
    static ref INPUT_BUF: UPSafeCell<VecDeque<u8>> =
        unsafe { UPSafeCell::new(VecDeque::with_capacity(128)) };
}

/// DW APB UART 寄存器偏移（字节偏移，和 rk3588_uart.rs 一致）
const UART2_BASE: usize = 0xfeb5_0000;
const UART_IER: usize = 0x04;
const UART_IIR: usize = 0x08;
const UART_LSR: usize = 0x14;
const UART_MSR: usize = 0x18;

/// 使能 UART RX 中断
pub fn enable_uart_rx_interrupt() {
    unsafe {
        // IER bit 0 = ERBFI (Enable Received Data Available Interrupt)
        // 只开 RX，不开 TX（避免 TX 中断风暴）
        ((UART2_BASE + UART_IER) as *mut u8).write_volatile(0x01);
    }

    // 诊断读取
    let ier = unsafe { ((UART2_BASE + UART_IER) as *const u8).read_volatile() };
    let iir = unsafe { ((UART2_BASE + UART_IIR) as *const u8).read_volatile() };
    let lsr = unsafe { ((UART2_BASE + UART_LSR) as *const u8).read_volatile() };
    log::info!("[Keyboard] UART RX interrupt enabled");
    log::info!("[UART] IER={:#04x} IIR={:#04x} LSR={:#04x}", ier, iir, lsr);
}

/// UART 中断处理 — 读 IIR 循环清除所有中断源
/// DW APB UART 的 IIR 和 16550 兼容：
///   bit 0: 0=有中断pending, 1=无中断
///   bits[3:1]: 中断类型
pub fn keyboard_interrupt_handler() {
    unsafe {
        loop {
            let iir = ((UART2_BASE + UART_IIR) as *const u8).read_volatile();
            // bit 0 = 1 表示没有更多 pending 中断
            if iir & 0x01 != 0 {
                break;
            }
            match (iir >> 1) & 0x07 {
                // 0b010 = RX 数据可用, 0b110 = 字符超时
                0b010 | 0b110 => {
                    while let Some(c) = crate::arch::driver::uart::getc() {
                        handle_char(c);
                    }
                }
                // 0b001 = TX 空（忽略）
                0b001 => {}
                // 0b011 = 线路状态错误（读 LSR 清除）
                0b011 => {
                    let _ = ((UART2_BASE + UART_LSR) as *const u8).read_volatile();
                }
                // 0b000 = Modem 状态（读 MSR 清除）
                0b000 => {
                    let _ = ((UART2_BASE + UART_MSR) as *const u8).read_volatile();
                }
                _ => break,
            }
        }
    }
}

/// 处理单个字符 — Ctrl 组合键转信号，其他入缓冲
fn handle_char(c: u8) {
    use crate::task::TASK_MANAER;
    use crate::task::Signal;

    match c {
        0x03 => {
            // Ctrl+C → SIGINT
           // TASK_MANAER.push_signal(Signal::SIGINT);
        }
        0x1C => {
            // Ctrl+\ → SIGQUIT
            // TASK_MANAER.push_signal(Signal::SIGQUIT);
        }
        0x1A => {
            // Ctrl+Z → SIGTSTP
            // TASK_MANAER.push_signal(Signal::SIGTSTP);
        }
        _ => {
            INPUT_BUF.lock().push_back(c);
        }
    }
}

/// 从输入缓冲区读取一个字符（非阻塞）
pub fn read_input() -> Option<u8> {
    INPUT_BUF.lock().pop_front()
}
