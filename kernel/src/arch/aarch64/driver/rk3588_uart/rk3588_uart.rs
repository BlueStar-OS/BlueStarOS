//! RK3588 UART 驱动
//! 基于 Synopsys DesignWare APB UART

use core::ptr::{read_volatile, write_volatile};

/// UART2 基地址（RK3588 默认控制台）
const UART2_BASE: usize = 0xfeb50000;

/// UART 寄存器偏移
const UART_RBR: usize = 0x00; // 接收缓冲寄存器（DLAB=0时，读取）
const UART_THR: usize = 0x00; // 发送保持寄存器（DLAB=0时，写入）
const UART_LSR: usize = 0x14; // 线路状态寄存器
const UART_LCR: usize = 0x0C; // 线路控制寄存器
const UART_DLL: usize = 0x00; // 波特率除数低位（DLAB=1时）
const UART_DLH: usize = 0x04; // 波特率除数高位（DLAB=1时）
const UART_IER: usize = 0x04; // 中断使能寄存器（DLAB=0时）
const UART_FCR: usize = 0x08; // FIFO 控制寄存器

/// 线路状态寄存器位定义
const LSR_DR: u8 = 1 << 0; // 数据就绪（接收FIFO非空）
const LSR_THRE: u8 = 1 << 5; // 发送保持寄存器空

/// RK3588 UART 驱动
pub struct Rk3588Uart {
    base: usize,
}

impl Rk3588Uart {
    /// 创建 UART 实例
    pub const fn new(base: usize) -> Self {
        Self { base }
    }

    /// 初始化 UART（简化版）
    pub fn init(&self, baudrate: u32) {
        unsafe {
            // 1. 禁用所有中断
            self.write_reg(UART_IER, 0x00);

            // 2. 设置 DLAB=1 以访问波特率寄存器
            let lcr = self.read_reg(UART_LCR);
            self.write_reg(UART_LCR, lcr | 0x80);

            // 3. 设置波特率（假设 UART 时钟为 24MHz）
            let divisor = 24_000_000 / (16 * baudrate);
            self.write_reg(UART_DLL, (divisor & 0xFF) as u8);
            self.write_reg(UART_DLH, ((divisor >> 8) & 0xFF) as u8);

            // 4. 设置数据格式：8N1
            self.write_reg(UART_LCR, 0x03);

            // 5. 使能并复位 FIFO
            self.write_reg(UART_FCR, 0x07);
        }
    }

    /// 发送一个字符
    pub fn putc(&self, c: u8) {
        unsafe {
            // 等待发送 FIFO 不满
            while (self.read_reg(UART_LSR) & LSR_THRE) == 0 {
                core::hint::spin_loop();
            }
            self.write_reg(UART_THR, c);
        }
    }

    /// 读取一个字符（非阻塞）
    /// 返回 Some(c) 如果有数据，否则返回 None
    pub fn getc(&self) -> Option<u8> {
        unsafe {
            // 检查接收 FIFO 是否有数据
            if (self.read_reg(UART_LSR) & LSR_DR) != 0 {
                Some(self.read_reg(UART_RBR))
            } else {
                None
            }
        }
    }

    /// 读取一个字符（阻塞）
    pub fn getc_blocking(&self) -> u8 {
        loop {
            if let Some(c) = self.getc() {
                return c;
            }
            core::hint::spin_loop();
        }
    }

    /// 发送字符串
    pub fn puts(&self, s: &str) {
        for byte in s.bytes() {
            if byte == b'\n' {
                self.putc(b'\r');
            }
            self.putc(byte);
        }
    }

    /// 写入字节数组
    pub fn write(&self, buf: &[u8]) {
        for &byte in buf {
            self.putc(byte);
        }
    }

    #[inline]
    unsafe fn read_reg(&self, offset: usize) -> u8 {
        read_volatile((self.base + offset) as *const u8)
    }

    #[inline]
    unsafe fn write_reg(&self, offset: usize, value: u8) {
        write_volatile((self.base + offset) as *mut u8, value)
    }
}

/// 全局 UART 实例
pub static UART: Rk3588Uart = Rk3588Uart::new(UART2_BASE);
