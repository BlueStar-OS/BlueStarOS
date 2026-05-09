use core::panic::PanicInfo;

use crate::{print, sys_exit};

#[panic_handler]
pub fn panic(info: &PanicInfo) -> ! {
    // 不同 Rust 版本下 `PanicInfo::message()` 的返回类型不完全一致：
    // 有的版本是 `Option<&fmt::Arguments>`，有的版本是 `PanicMessage`。
    // 这里统一直接打印整个 `PanicInfo` 的 Debug 结果，避免绑定具体接口形态。
    let panic_location = info.location();
    if let Some(location) = panic_location {
        print!(
            "USER APPLICATION panic on file:{} line:{} detail:{:?} \n",
            location.file(),
            location.line(),
            info
        )
    } else {
        print!("USER APPLICATION panic detail:{:?}", info)
    }
    sys_exit(1);
}
