use core::fmt;
use crate::fs::component::stdio::stdio::print as driver_print;

/// 打印函数（委托给driver模块）
pub fn print(fmt: fmt::Arguments) {
    driver_print(fmt);
}


///print string
#[macro_export]
macro_rules! print {
    ($fmt: literal $(, $($arg: tt)+)?) => {
        $crate::console::print(format_args!($fmt $(, $($arg)+)?))
    }
}
/// Println! to the host console using the format string and arguments.
#[macro_export]
macro_rules! println {
    ($fmt: literal $(, $($arg: tt)+)?) => {
        $crate::console::print(format_args!(concat!($fmt, "\n") $(, $($arg)+)?))
    }
}