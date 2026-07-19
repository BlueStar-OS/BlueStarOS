use crate::fs::component::tty::print as driver_print;
use core::fmt;

/// 内核打印函数,直接输出到当前任务缓冲区？？，不对吧。
pub fn kprint(fmt: fmt::Arguments) {
    driver_print(fmt);
}

/// 内核格式化打印宏（不带换行）。
///
/// 用法: `kprint!("value = {:#x}", val);`
#[macro_export]
macro_rules! kprint {
    ($fmt: literal $(, $($arg: tt)+)?) => {
        $crate::console::kprint(format_args!($fmt $(, $($arg)+)?))
    }
}

/// 内核格式化打印宏（带换行）。
///
/// 用法: `kprintln!("Hello, kernel!");`
#[macro_export]
macro_rules! kprintln {
    ($fmt: literal $(, $($arg: tt)+)?) => {
        $crate::console::kprint(format_args!(concat!($fmt, "\n") $(, $($arg)+)?))
    }
}
