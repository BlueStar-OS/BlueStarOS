use core::panic::PanicInfo;
use log::error;

use crate::sbi::shutdown;
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    let location = _info.location();
    if let Some(loca) = location {
        error!(
            "[Kernel Panic]: Kernel panic at {}:{}: {}",
            loca.file(),
            loca.line(),
            _info.message()
        )
    }else {
        error!("[Kernel Panic]: Kernel panic: {}", _info.message());
    }

    shutdown()
}
