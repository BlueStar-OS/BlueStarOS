// QEMU UART串口驱动
const UART0_BASE: usize = 0x10000000;

/// 发送一个字符
pub fn putc(c: u8) {
    unsafe {
        let uart = UART0_BASE as *mut u8;
        uart.write_volatile(c);
    }
}

/// 读取一个字符（非阻塞）
/// 返回 Some(c) 如果有数据，否则返回 None
pub fn getc() -> Option<u8> {
    unsafe {
        let uart = UART0_BASE as *const u8;
        // QEMU简化UART：直接读取即可
        // 实际硬件需要检查状态寄存器
        Some(uart.read_volatile())
    }
}

/// 读取一个字符（阻塞）
pub fn getc_blocking() -> u8 {
    loop {
        if let Some(c) = getc() {
            return c;
        }
        core::hint::spin_loop();
    }
}