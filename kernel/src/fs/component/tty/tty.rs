use core::fmt::{self, Write};
use alloc::sync::Arc;
use log::error;
use crate::task::TASK_MANAER;
use crate::fs::vfs::{File, OpenFlags, VfsFsError};
use crate::arch::driver::uart;
#[cfg(target_arch = "riscv64")]
use riscv::register::sstatus;
#[cfg(target_arch = "riscv64")]
use riscv::register::sie;


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
    ///调用栈顶必须为traphandler！！！，因为其中有TASK_MANAER.suspend_and_run_task();
    pub fn get_char() -> u8 {
        loop {
            // 先检查中断缓冲区
            #[cfg(target_arch = "riscv64")]
            if let Some(c) = crate::arch::driver::keyboard::read_input() {
                return c;
            }
            #[cfg(target_arch = "aarch64")]
            if let Some(c) = crate::arch::driver::keyboard::read_input() {
                return c;
            }
            // 非 riscv64/aarch64：轮询
            #[cfg(not(any(target_arch = "riscv64", target_arch = "aarch64")))]
            if let Some(cha) = uart::getc() {
                return cha;
            }
            // 等待中断：开全局中断 + wfi
            #[cfg(target_arch = "riscv64")]
            unsafe {
                sstatus::set_sie();    // 开全局中断
                core::arch::asm!("wfi"); //等待中断
                sstatus::clear_sie();  // 关全局中断
            }
            #[cfg(target_arch = "aarch64")]
            unsafe {
                core::arch::asm!("msr daifclr, #2"); // 开 IRQ
                core::arch::asm!("wfi");              // 等待中断
                core::arch::asm!("msr daifset, #2"); // 关 IRQ
            }
            // wfi 返回后让出 CPU
            TASK_MANAER.suspend_and_run_task();
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
