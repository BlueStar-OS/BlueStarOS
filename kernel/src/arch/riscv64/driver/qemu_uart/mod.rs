// QEMU UART串口驱动
#[cfg(target_arch = "riscv64")]
static mut UART0_BASE: usize = 0x10000000;
#[cfg(target_arch = "aarch64")]
static mut UART0_BASE: usize = 0x09000000;

use crate::kprintln;

/// 发送一个字符
pub fn putc(c: u8) {
    unsafe {
        let uart = UART0_BASE as *mut u8;
        uart.write_volatile(c);
    }
}

/// 读取一个字符（非阻塞）
/// 返回 Some(c) 如果有数据，否则返回 None
/// 16550 UART: 先检查 LSR (偏移+5) 的 DR 位 (bit 0)，
/// 有数据才读 RBR (偏移+0)，读 RBR 会自动清除 DR
pub fn getc() -> Option<u8> {
    unsafe {
        let lsr = (UART0_BASE + 5) as *const u8;
        let rbr = UART0_BASE as *const u8;
        // LSR bit 0 = Data Ready
        if lsr.read_volatile() & 1 != 0 {
            Some(rbr.read_volatile())
        } else {
            None
        }
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

// ===== DTB 探测器示例 =====

use crate::driver::dtb::DeviceNode;
use log::info;

/// UART 16550 探测器
fn uart_16550_probe(node: &DeviceNode, _compatible: &str) -> Result<(), &'static str> {
    // 获取寄存器地址
    let reg = node.get_property("reg").ok_or("Missing reg property")?;
    let regs = reg.as_reg(2, 2);

    if regs.is_empty() {
        return Err("Empty reg property");
    }

    let base_addr = regs[0].address as usize;
    kprintln!("[UART Probe] Found UART at {:#x}, size={:#x}", base_addr, regs[0].size);

    unsafe{
        UART0_BASE = base_addr
    }

    // 注册 QEMU MMIO 区域（UART + PLIC + virtio）
    {
        use crate::memory::{register_kernel_mmio, VirNumRange, MapAreaFlags};
        use crate::arch::memory::VirAddr;

        let mmio_range = VirNumRange::new(VirAddr(base_addr), VirAddr(base_addr+(regs[0].size as usize)-1));
        let flags = MapAreaFlags::V | MapAreaFlags::R | MapAreaFlags::W
                  | MapAreaFlags::A | MapAreaFlags::G | MapAreaFlags::DEV;
        register_kernel_mmio(mmio_range, flags);
    }

    Ok(())
}

// 注册 UART 探测器
crate::dtb_probe! {
    compatible: "ns16550a",
    priority: Mid,
    driver: "uart-16550",
    probe: uart_16550_probe
}



//UART drivers
pub mod uart {
    pub use super::{putc, getc, getc_blocking};
}