use core::fmt;
use crate::fs::component::stdio::stdio::print as driver_print;

/// 内核打印函数,直接输出到当前任务缓冲区？？，不对吧。
pub fn kprint(fmt: fmt::Arguments) {
    driver_print(fmt);
}

#[macro_export]
macro_rules! kprint {
    ($fmt: literal $(, $($arg: tt)+)?) => {
        $crate::console::kprint(format_args!($fmt $(, $($arg)+)?))
    }
}

#[macro_export]
macro_rules! kprintln {
    ($fmt: literal $(, $($arg: tt)+)?) => {
        $crate::console::kprint(format_args!(concat!($fmt, "\n") $(, $($arg)+)?))
    }
}