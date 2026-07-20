// 键盘驱动（通过 UART 中断实现）
// 捕获 Ctrl 组合键并转换为信号投递到当前任务

use crate::arch::driver::uart;
use crate::sync::UPSafeCell;
use crate::task::signal::{push_signal, OsSignal};
use crate::task::{Signal, TASK_MANAER};
use alloc::collections::vec_deque::VecDeque;
use lazy_static::lazy_static;
/// 输入字符缓冲区（非信号字符存这里，供 TTY 消费）
lazy_static! {
    static ref INPUT_BUF: UPSafeCell<VecDeque<u8>> = UPSafeCell::new(VecDeque::with_capacity(256));
}

/// UART 16550 寄存器地址
const UART_BASE: usize = 0x1000_0000;
const UART_IIR: usize = UART_BASE + 2;
const UART_LSR: usize = UART_BASE + 5;
const UART_MSR: usize = UART_BASE + 6;

/// 使能 UART RX 中断（16550 IER 寄存器 bit 0）
pub fn enable_uart_rx_interrupt() {
    #[cfg(target_arch = "riscv64")]
    {
        let ier = (UART_BASE + 1) as *mut u8;
        unsafe {
            // 只开 RX 中断(bit0)，不要 OR 保留其他位
            ier.write_volatile(0x01);
        }
    }
    crate::kprintln!("[Keyboard] UART RX interrupt enabled");
    // 诊断：读回 UART 寄存器确认状态
    unsafe {
        let ier_val = ((UART_BASE + 1) as *const u8).read_volatile();
        let iir_val = (UART_IIR as *const u8).read_volatile();
        let lsr_val = (UART_LSR as *const u8).read_volatile();
        crate::kprintln!(
            "[UART] IER={:#04x} IIR={:#04x} LSR={:#04x}",
            ier_val,
            iir_val,
            lsr_val
        );
    }
}

/// UART 中断处理入口
/// 16550 是电平触发，必须读 IIR 排空所有中断源，否则 IRQ 线不释放
pub fn keyboard_interrupt_handler() {
    unsafe {
        loop {
            let iir = (UART_IIR as *const u8).read_volatile();
            // bit0=1 表示无中断挂起，可以退出
            if iir & 0x01 != 0 {
                break;
            }
            match (iir >> 1) & 0x07 {
                // RX 数据可用 或 字符超时
                0b010 | 0b110 => {
                    while let Some(c) = uart::getc() {
                        handle_char(c);
                    }
                }
                // TX 持有寄存器空 —— 读 IIR 已清除
                0b001 => {}
                // 接收线路状态错误 —— 读 LSR 清除
                0b011 => {
                    let _ = (UART_LSR as *const u8).read_volatile();
                }
                // Modem 状态变化 —— 读 MSR 清除
                0b000 => {
                    let _ = (UART_MSR as *const u8).read_volatile();
                }
                _ => break,
            }
        }
    }
}

/// 处理单个字符
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

/// 从输入缓冲区读取一个字符（供 TTY 使用）
pub fn read_input() -> Option<u8> {
    let mut re = None;
    INPUT_BUF.try_lock(|lock| {
        if let Some(buf) = lock {
            re = buf.pop_front();
        }
    });
    re
}

/// 将信号投递到当前任务的信号队列
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
