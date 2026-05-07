// PLIC (Platform-Level Interrupt Controller) 驱动
// QEMU riscv64 virt 平台专用

use crate::arch::memory::*;
use crate::dtb::DeviceNode;
use crate::dtb_probe;
use crate::kprintln;
use crate::register_kernel_mmio;
use crate::MapAreaFlags;
use crate::VirNumRange;
/// QEMU virt PLIC 基地址
const PLIC_BASE: usize = 0x0C00_0000;

/// UART0 中断号
pub const UART0_IRQ: u32 = 10;

/// S-mode context 编号 = 1（QEMU virt: context 0 = M-mode, context 1 = S-mode）
const S_CONTEXT: usize = 1;

/// 中断源优先级寄存器: base + irq * 4
const fn priority_reg(irq: u32) -> usize {
    PLIC_BASE + (irq as usize) * 4
}

/// S-mode 使能寄存器: base + 0x2000 + context * 0x80
const fn senable_reg(context: usize) -> usize {
    PLIC_BASE + 0x2000 + context * 0x80
}

/// S-mode 阈值寄存器: base + 0x20_0000 + context * 0x1000
const fn sthreshold_reg(context: usize) -> usize {
    PLIC_BASE + 0x20_0000 + context * 0x1000
}

/// S-mode claim/complete 寄存器: threshold + 4
const fn sclaim_reg(context: usize) -> usize {
    sthreshold_reg(context) + 4
}

/// 初始化 PLIC：使能 UART0 中断，设置优先级和阈值
pub fn plic_init() {
    unsafe {
        // 设置 UART0 优先级为 1（>0 即可触发）
        let prio = priority_reg(UART0_IRQ) as *mut u32;
        prio.write_volatile(1);

        // 使能 UART0 中断（bit 10）
        let enable = senable_reg(S_CONTEXT) as *mut u32;
        let val = enable.read_volatile();
        enable.write_volatile(val | (1 << UART0_IRQ));

        // 设置 S-mode 阈值为 0（接受所有优先级 > 0 的中断）
        let threshold = sthreshold_reg(S_CONTEXT) as *mut u32;
        threshold.write_volatile(0);
    }
    crate::kprintln!("[PLIC] Initail success, UART0 IRQ {} enabled!", UART0_IRQ);
}

/// 读取 claim 寄存器，获取当前待处理的中断号
/// 返回 0 表示无中断
pub fn plic_claim() -> u32 {
    unsafe {
        let claim = sclaim_reg(S_CONTEXT) as *const u32;
        claim.read_volatile()
    }
}

/// 写 complete 寄存器，通知 PLIC 中断处理完成
pub fn plic_complete(irq: u32) {
    unsafe {
        let complete = sclaim_reg(S_CONTEXT) as *mut u32;
        complete.write_volatile(irq);
    }
}

pub fn plic_fn(node: &DeviceNode, _compatible: &str) -> Result<(), &'static str> {
    kprintln!("[PLIC]:PLIC device is probed!, registe mmio...");
    let reg = node.get_property("reg").ok_or("Missing reg property")?;
    let regs = reg.as_reg(2, 2);
    if regs.is_empty() {
        return Err("Empty reg property");
    }

    let base_addr = regs[0].address as usize;
    let size = regs[0].size as usize;
    if size == 0 {
        return Err("plic MMIO size is zero");
    }

    let mmio_range = VirNumRange::new(VirAddr(base_addr), VirAddr(base_addr + size - 1));
    let flags = MapAreaFlags::V
        | MapAreaFlags::R
        | MapAreaFlags::W
        | MapAreaFlags::A
        | MapAreaFlags::G
        | MapAreaFlags::DEV;
    register_kernel_mmio(mmio_range, flags);

    Ok(())
}

dtb_probe! {
    compatible: "sifive,plic-1.0.0",
    priority: High,
    driver: "plic",
    probe: plic_fn
}
