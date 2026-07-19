use crate::arch::driver::uart;
use crate::arch::memory::{PageTable, VirAddr};
use crate::arch::{enable_irq, wait_for_interrupt};
use crate::driver::gpu::vga_console::{vga_screen, vga_screen_mut};
use crate::fs::vfs::{File, OpenFlags, VfsFsError};
use crate::task::TASK_MANAER;
use alloc::sync::Arc;
use core::fmt::{self, Write};
use log::error;
pub const FD_TYPE_STDIN: usize = 0;
pub const FD_TYPE_STDOUT: usize = 1;
pub const FD_TYPE_STDERR: usize = 2;

/// ioctl 命令号：获取终端窗口尺寸。
/// 参考: linux-5.4.29/include/uapi/asm-generic/ioctls.h:38
const TIOCGWINSZ: u32 = 0x5413;

/// 标准输出文件节点
pub struct Stdout;

/// 标准输入文件节点
pub struct Stdin;

/// TTY窗口信息
#[repr(C)]
struct tty_winsize {
    ws_row: u16,    // 行数（终端高度，字符单位）
    ws_col: u16,    // 列数（终端宽度，字符单位）
    ws_xpixel: u16, // 水平像素数（可选，可能为0）
    ws_ypixel: u16, // 垂直像素数（可选，可能为0）
}

///标准错误文件节点
pub struct Stderr;

impl Stdin {
    #[inline]
    pub(crate) fn poll_input() -> Option<u8> {
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
        // 开中断
        enable_irq();
        wait_for_interrupt();

        TASK_MANAER.suspend_and_run_task();
    }

    /// `suspend_and_run_task()` 只能在 trap 顶层安全调用。
    /// AArch64 键盘 IRQ 会在 syscall 的内核栈上唤醒 `wfi`，这里不能直接调度。
    pub(crate) fn get_char() -> u8 {
        loop {
            if let Some(c) = Self::poll_input() {
                return c;
            }
            Self::wait_input_event();
        }
    }
}

impl File for Stdout {
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }

    fn read(&self, _buf: &mut [u8]) -> Result<usize, VfsFsError> {
        Err(VfsFsError::NotSupported)
    }

    fn write(&self, buf: &[u8]) -> Result<usize, VfsFsError> {
        for &byte in buf {
            uart::putc(byte);
            unsafe {
                if !vga_screen().fb_base.is_null() {
                    vga_screen_mut().draw_char(byte as char)
                }
            }
        }
        Ok(buf.len())
    }
}

impl File for Stdin {
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }

    fn read(&self, buf: &mut [u8]) -> Result<usize, VfsFsError> {
        let mut read_count = 0usize;
        buf.iter_mut().for_each(|b| *b = 0);

        for slot in buf.iter_mut() {
            let mut cha = Self::get_char();
            while cha == 0 {
                // TODO(lock-invariant): suspend_and_run_task must not run under task_que_inner lock. Document and enforce this invariant.
                TASK_MANAER.suspend_and_run_task();
                cha = Self::get_char();
            }
            *slot = cha;
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

    fn ioctl(&self, cmd: u32, arg: usize) -> Result<usize, VfsFsError> {
        match cmd {
            TIOCGWINSZ => {
                // 从 VGA console 读取当前终端的字符行数和列数
                let (cols, rows) =
                    unsafe { (vga_screen().cols() as u16, vga_screen().rows() as u16) };
                let ws = tty_winsize {
                    ws_row: rows,
                    ws_col: cols,
                    ws_xpixel: 0,
                    ws_ypixel: 0,
                };

                // 通过页表将 winsize 写入用户空间
                let satp = TASK_MANAER.get_current_stap();
                let mut tb = PageTable::crate_table_from_satp(satp);
                let pa = tb.translate(VirAddr(arg));
                if pa.is_none() {
                    return Err(VfsFsError::NotSupported);
                }
                // SAFETY: translate 已验证 arg 指向用户空间可写页面，
                // winsize 结构体 (8 字节) 不会跨页，直接写入物理地址。
                unsafe {
                    *(pa.unwrap().0 as *mut tty_winsize) = ws;
                }
                Ok(0)
            }
            _ => Err(VfsFsError::NotSupported),
        }
    }
}

impl File for Stderr {
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }

    fn read(&self, _buf: &mut [u8]) -> Result<usize, VfsFsError> {
        Err(VfsFsError::NotSupported)
    }

    fn write(&self, buf: &[u8]) -> Result<usize, VfsFsError> {
        for &b in b"<3>" {
            uart::putc(b);
        }
        for &byte in buf {
            uart::putc(byte);
        }
        Ok(buf.len())
    }
}

impl Write for Stdout {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let _ = self.write(s.as_bytes());
        Ok(())
    }
}

/// 打印函数
pub fn print(fmt: fmt::Arguments) {
    let mut stdout = Stdout;
    stdout.write_fmt(fmt).unwrap();
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
