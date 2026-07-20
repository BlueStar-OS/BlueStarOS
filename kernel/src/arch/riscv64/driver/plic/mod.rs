//! PLIC (Platform-Level Interrupt Controller) 驱动
//! QEMU riscv64 virt 平台专用
//!
//! 提供全局中断注册表 + 注册/分发接口。
//! 使用方式:
//! ```ignore
//! plic_init();                              // 设置 S-mode 阈值
//! register_irq(IRQ_NUM, my_handler, 1);     // 注册中断
//! // trap handler 中:
//! dispatch_irq();                           // claim → 查表 → handler → complete
//! ```

use lazy_static::lazy_static;
use log::warn;

use crate::arch::memory::*;
use crate::dtb::DeviceNode;
use crate::dtb_probe;
use crate::kprintln;
use crate::register_kernel_mmio;
use crate::sync::UPSafeCell;
use crate::MapAreaFlags;
use crate::VirNumRange;

/// QEMU virt PLIC 基地址
const PLIC_BASE: usize = 0x0C00_0000;

/// QEMU virt 平台最大中断源数量 (IRQ 1-52, 0=无中断)
const MAX_IRQ: usize = 53;

/// 外部中断处理函数类型
pub type IrqHandler = fn();

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

// ============================================================================
// 全局中断注册表
// ============================================================================

lazy_static! {
    /// 全局 IRQ → handler 映射表
    /// 索引 = IRQ 号, None = 未注册
    static ref PLIC_HANDLERS: UPSafeCell<[Option<IrqHandler>; MAX_IRQ]> =
        UPSafeCell::new([None; MAX_IRQ]);
}

/// 注册中断处理函数
///
/// ## 参数
/// - `irq`:      中断号 (1..MAX_IRQ-1)
/// - `handler`:  中断处理函数指针
/// - `priority`: 硬件优先级 (0=从不触发, 1-7 有效)
///
/// 执行操作:
/// 1. 写 PLIC 优先级寄存器
/// 2. 使能 S-mode 对该 IRQ 的中断位
/// 3. 注册 handler 到全局表
pub fn register_irq(irq: u32, handler: IrqHandler, priority: u8) {
    assert!((irq as usize) < MAX_IRQ, "plic: invalid IRQ number {}", irq,);
    assert!(irq > 0, "plic: IRQ 0 is reserved (no interrupt)");

    // 1. 写 PLIC 优先级寄存器
    unsafe {
        let prio = priority_reg(irq) as *mut u32;
        prio.write_volatile(priority as u32);
    }

    // 2. 使能 S-mode 对该 IRQ 的中断
    //
    // PLIC enable 数组按每 32 个 source 拆分到独立的 32-bit word 中。
    // IRQ N 的 enable bit 位于 word[N/32] 的 bit[N%32]。
    // 参考 QEMU sifive_plic.c:104:
    //   `atomic_set_masked(&plic->pending[irq >> 5], 1 << (irq & 31), ...)`
    unsafe {
        let word_index = (irq as usize) / 32;
        let bit = (irq as usize) % 32;
        let enable = (senable_reg(S_CONTEXT) + word_index * 4) as *mut u32;
        let val = enable.read_volatile();
        enable.write_volatile(val | (1 << bit));
    }

    // 3. 注册 handler 到全局表
    PLIC_HANDLERS.lock(|x| x[irq as usize] = Some(handler));
}

/// 中断分发入口
///
/// 由 trap handler 在 `SupervisorExternal` 时调用。
/// 流程: claim → 查表 → 调用 handler → complete。
///
/// 未注册的中断会被记录警告并 complete。
pub fn dispatch_irq() {
    let irq = plic_claim();
    if irq == 0 {
        return; // spurious interrupt
    }
    if (irq as usize) >= MAX_IRQ {
        warn!("plic: IRQ {} out of range", irq);
        plic_complete(irq);
        return;
    }

    let handler = PLIC_HANDLERS.lock(|x| x[irq as usize]);
    match handler {
        Some(h) => h(),
        None => warn!("plic: unregistered IRQ {}", irq),
    }

    plic_complete(irq);
}

/// 初始化 PLIC：设置 S-mode 阈值为 0（接受所有优先级 > 0 的中断）
///
/// 具体设备的中断通过 `register_irq()` 在各自驱动初始化时注册。
pub fn plic_init() {
    unsafe {
        // 设置 S-mode 阈值为 0（接受所有优先级 > 0 的中断）
        let threshold = sthreshold_reg(S_CONTEXT) as *mut u32;
        threshold.write_volatile(0);
    }
    kprintln!("[PLIC] Initial success! threshold=0, ready for IRQ registration.");
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
