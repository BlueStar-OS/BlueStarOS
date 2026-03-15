pub mod rk3588_uart;
pub use rk3588_uart::*;




/// 香橙派5，rk3500全局uart设备
pub mod uart {
    use super::rk3588_uart::*;
    
    pub fn putc(c: u8) {
        UART.putc(c);
    }

    pub fn getc() -> Option<u8> {
        UART.getc()
    }

    pub fn getc_blocking() -> u8 {
        UART.getc_blocking()
    }
}