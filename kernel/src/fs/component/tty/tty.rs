use crate::arch::driver::uart;
use crate::arch::{disable_irq, enable_irq, wait_for_interrupt};
use crate::disable_timer_interrupt;
use crate::enable_timer_interrupt;
use crate::fs::vfs::{File, OpenFlags, VfsFsError};
use crate::info;
use crate::task::TASK_MANAER;
use alloc::sync::Arc;
use core::fmt::{self, Write};
use log::{error, warn};
pub const FD_TYPE_STDIN: usize = 0;
pub const FD_TYPE_STDOUT: usize = 1;
pub const FD_TYPE_STDERR: usize = 2;

/// 标准输出文件节点
pub struct Stdout;

/// 标准输入文件节点
pub struct Stdin;

///标准错误文件节点
pub struct Stderr;

impl Stdin {
    #[inline]
    fn poll_input() -> Option<u8> {
        #[cfg(any(target_arch = "riscv64", target_arch = "aarch64"))]
        {
            crate::arch::driver::keyboard::read_input()
        }
        #[cfg(not(any(target_arch = "riscv64", target_arch = "aarch64")))]
        {
            uart::getc()
        }
    }

    #[inline]
    fn wait_input_event() {
        // 关时钟
        disable_timer_interrupt();
        // 开中断
        enable_irq();
        wait_for_interrupt();
        // 关中断
        disable_irq();
        // 开时钟
        enable_timer_interrupt();

        TASK_MANAER.suspend_and_run_task();
    }

    /// `suspend_and_run_task()` 只能在 trap 顶层安全调用。
    /// AArch64 键盘 IRQ 会在 syscall 的内核栈上唤醒 `wfi`，这里不能直接调度。
    pub fn get_char() -> u8 {
        loop {
            if let Some(c) = Self::poll_input() {
                return c;
            }
            Self::wait_input_event();
        }
    }
}

impl File for Stdout {
    fn read(&self, _buf: &mut [u8]) -> Result<usize, VfsFsError> {
        Err(VfsFsError::NotSupported)
    }

    fn write(&self, buf: &[u8]) -> Result<usize, VfsFsError> {
        for &byte in buf {
            uart::putc(byte);
        }
        Ok(buf.len())
    }
}

impl File for Stdin {
    fn read(&self, buf: &mut [u8]) -> Result<usize, VfsFsError> {
        let mut read_count = 0usize;
        buf.iter_mut().for_each(|b| *b = 0);

        for slot in buf.iter_mut() {
            let mut cha = Self::get_char();
            while cha == 0 {
                TASK_MANAER.suspend_and_run_task();
                cha = Self::get_char();
            }
            *slot = cha as u8;
            read_count += 1;
            if *slot == 13 {
                break;
            }
        }
        Ok(read_count)
    }

    fn write(&self, _buf: &[u8]) -> Result<usize, VfsFsError> {
        Err(VfsFsError::NotSupported)
    }
}

impl File for Stderr {
    fn read(&self, _buf: &mut [u8]) -> Result<usize, VfsFsError> {
        Err(VfsFsError::NotSupported)
    }

    fn write(&self, buf: &[u8]) -> Result<usize, VfsFsError> {
        for &b in b"<3>" {
            uart::putc(b as u8);
        }
        for &byte in buf {
            uart::putc(byte as u8);
        }
        Ok(buf.len())
    }
}

impl Write for Stdout {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for cha in s.chars() {
            uart::putc(cha as u8);
        }
        Ok(())
    }
}

/// 打印函数
pub fn print(fmt: fmt::Arguments) {
    let mut stdout = Stdout;
    stdout.write_fmt(fmt).unwrap()
}

pub fn stdin_file() -> Arc<dyn File> {
    let _ = OpenFlags::empty();
    Arc::new(Stdin)
}

pub fn stdout_file() -> Arc<dyn File> {
    let _ = OpenFlags::WRONLY;
    Arc::new(Stdout)
}

pub fn stderr_file() -> Arc<dyn File> {
    let _ = OpenFlags::WRONLY;
    Arc::new(Stderr)
}
