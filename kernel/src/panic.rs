use core::panic::PanicInfo;
use log::error;

use crate::shutdown;
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    let location = _info.location();
    if let Some(loca) = location {
        crate::kprintln!(
            "[Kernel Panic]: {}:{}: {:?}",
            loca.file(),
            loca.line(),
            _info.message()
        );
    } else {
        crate::kprintln!("[Kernel Panic]: {:?}", _info.message());
    }

    shutdown()
}
