//! GICv3 驱动 — Distributor + Redistributor + CPU Interface
//!
//! 参考 Linux: drivers/irqchip/irq-gic-v3.c
//! RK3588 地址: GICD=0xfe600000, GICR=0xfe680000

use core::arch::asm;
use crate::kprintln;

use super::{gicd_base, gicr_rd_base, gicr_sgi_base};

// ==================== MMIO 辅助函数 ====================

#[inline]
fn write32(addr: usize, val: u32) {
    unsafe { (addr as *mut u32).write_volatile(val); }
}

#[inline]
fn read32(addr: usize) -> u32 {
    unsafe { (addr as *const u32).read_volatile() }
}

#[inline]
fn write64(addr: usize, val: u64) {
    unsafe { (addr as *mut u64).write_volatile(val); }
}

#[inline]
fn read_mpidr_affinity() -> u64 {
    let mpidr: u64;
    unsafe { asm!("mrs {}, mpidr_el1", out(reg) mpidr); }
    ((mpidr >> 32) & 0xFF) << 32
        | ((mpidr >> 16) & 0xFF) << 16
        | ((mpidr >> 8) & 0xFF) << 8
        | (mpidr & 0xFF)
}

// ==================== GICD 寄存器偏移 ====================

const GICD_CTLR: usize = 0x0000;
const GICD_TYPER: usize = 0x0004;
const GICD_IGROUPR: usize = 0x0080;
const GICD_ISENABLER: usize = 0x0100;
const GICD_ICENABLER: usize = 0x0180;
const GICD_IPRIORITYR: usize = 0x0400;
const GICD_ICFGR: usize = 0x0C00;
const GICD_IROUTER: usize = 0x6000;

// GICD_CTLR 位域
const GICD_CTLR_RWP: u32 = 1 << 31;
const GICD_CTLR_ARE_NS: u32 = 1 << 4;
const GICD_CTLR_ENABLE_G1A: u32 = 1 << 1;
const GICD_CTLR_ENABLE_G1: u32 = 1 << 0;

// ==================== GICR 寄存器偏移 ====================

const GICR_CTLR: usize = 0x0000;
const GICR_WAKER: usize = 0x0014;
// SGI_base 帧（+0x10000）
const GICR_IGROUPR0: usize = 0x0080;
const GICR_ISENABLER0: usize = 0x0100;
const GICR_ICENABLER0: usize = 0x0180;
const GICR_IPRIORITYR0: usize = 0x0400;

const GICR_WAKER_PROCESSOR_SLEEP: u32 = 1 << 1;
const GICR_WAKER_CHILDREN_ASLEEP: u32 = 1 << 2;
const GICR_CTLR_RWP: u32 = 1 << 3;

// ==================== ICC 系统寄存器操作 ====================

/// 读 ICC_IAR1_EL1 — claim 当前最高优先级 pending 中断
#[inline]
pub fn gic_read_iar() -> u32 {
    let irqnr: u64;
    unsafe {
        asm!("mrs {}, S3_0_C12_C12_0", out(reg) irqnr);
        asm!("dsb sy");
    }
    irqnr as u32
}

/// 写 ICC_EOIR1_EL1 — 通知 GIC 中断处理完成
#[inline]
pub fn gic_write_eoir(irqnr: u32) {
    unsafe {
        asm!("msr S3_0_C12_C12_1, {}", in(reg) irqnr as u64);
        asm!("isb");
    }
}

// ==================== Distributor 初始化 ====================

fn gic_dist_wait_for_rwp() {
    let base = gicd_base();
    while read32(base + GICD_CTLR) & GICD_CTLR_RWP != 0 {
        core::hint::spin_loop();
    }
}

/// 初始化 Distributor（全局一次）
/// 参考 Linux gic_dist_init() — irq-gic-v3.c:914
fn gic_dist_init() {
    let base = gicd_base();

    // 1. 关闭 Distributor
    write32(base + GICD_CTLR, 0);
    gic_dist_wait_for_rwp();

    // 2. 读取支持的中断数量
    let typer = read32(base + GICD_TYPER);
    let it_lines = (((typer & 0x1f) + 1) * 32) as usize;
    kprintln!("[GIC] GICD_TYPER={:#x}, max INTID={}", typer, it_lines);

    // 3. 配置所有 SPI 为 Non-Secure Group 1
    for i in (32..it_lines).step_by(32) {
        write32(base + GICD_IGROUPR + i / 8, 0xFFFF_FFFF);
    }

    // 4. 禁用所有 SPI
    for i in (32..it_lines).step_by(32) {
        write32(base + GICD_ICENABLER + i / 8, 0xFFFF_FFFF);
    }

    // 5. 设置所有 SPI 优先级为默认值 (0xa0)
    for i in (32..it_lines).step_by(4) {
        write32(base + GICD_IPRIORITYR + i, 0xa0a0_a0a0);
    }

    // 6. 设置所有 SPI 为电平触发
    for i in (32..it_lines).step_by(16) {
        write32(base + GICD_ICFGR + i / 4, 0);
    }

    // 7. 设置所有 SPI 路由到当前 CPU
    let affinity = read_mpidr_affinity();
    for i in 32..it_lines {
        write64(base + GICD_IROUTER + i * 8, affinity);
    }

    // 8. 启用 Distributor: ARE_NS | EnableGrp1A | EnableGrp1
    write32(base + GICD_CTLR, GICD_CTLR_ARE_NS | GICD_CTLR_ENABLE_G1A | GICD_CTLR_ENABLE_G1);
    gic_dist_wait_for_rwp();

    kprintln!("[GIC] Distributor initialized, {} interrupt lines", it_lines);
}

// ==================== Redistributor 初始化 ====================

fn gic_redist_wait_for_rwp(rd_base: usize) {
    while read32(rd_base + GICR_CTLR) & GICR_CTLR_RWP != 0 {
        core::hint::spin_loop();
    }
}

/// 初始化 Redistributor（per-CPU）
/// 参考 Linux gic_cpu_init() — irq-gic-v3.c:1270
fn gic_redist_init(cpu: usize) {
    let rd_base = gicr_rd_base(cpu);
    let sgi_base = gicr_sgi_base(cpu);

    // 1. 唤醒 Redistributor
    let mut waker = read32(rd_base + GICR_WAKER);
    waker &= !GICR_WAKER_PROCESSOR_SLEEP;
    write32(rd_base + GICR_WAKER, waker);

    // 等待 ChildrenAsleep 变为 0
    let mut timeout = 1000_000u32;
    while read32(rd_base + GICR_WAKER) & GICR_WAKER_CHILDREN_ASLEEP != 0 {
        timeout -= 1;
        if timeout == 0 {
            kprintln!("[GIC] GICR wakeup timeout for CPU{}", cpu);
            break;
        }
        core::hint::spin_loop();
    }

    // 2. 配置所有 SGI/PPI 为 Non-Secure Group 1
    write32(sgi_base + GICR_IGROUPR0, 0xFFFF_FFFF);

    // 3. 设置 SGI/PPI 默认优先级
    for i in (0..32usize).step_by(4) {
        write32(sgi_base + GICR_IPRIORITYR0 + i, 0xa0a0_a0a0);
    }

    // 4. 禁用所有 SGI/PPI（后面按需开启）
    write32(sgi_base + GICR_ICENABLER0, 0xFFFF_FFFF);
    gic_redist_wait_for_rwp(rd_base);

    kprintln!("[GIC] Redistributor CPU{} initialized", cpu);
}

// ==================== CPU Interface 初始化 ====================

/// 初始化 CPU Interface（per-CPU，系统寄存器）
/// 参考 Linux gic_cpu_sys_reg_init() — irq-gic-v3.c:1135
fn gic_cpu_interface_init() {
    unsafe {
        // 1. 启用系统寄存器接口 ICC_SRE_EL1.SRE = 1
        let sre: u64;
        asm!("mrs {}, S3_0_C12_C12_5", out(reg) sre);
        if sre & 1 == 0 {
            asm!("msr S3_0_C12_C12_5, {}", in(reg) sre | 1);
            asm!("isb");
        }

        // 2. 设置优先级掩码 — 允许所有优先级
        asm!("msr S3_0_C4_C6_0, {}", in(reg) 0xFFu64);  // ICC_PMR_EL1

        // 3. BPR1 = 0（最细粒度优先级分组）
        asm!("msr S3_0_C12_C12_3, {}", in(reg) 0u64);  // ICC_BPR1_EL1

        // 4. EOI 模式 = 0（写 EOIR 同时 deactivate）
        asm!("msr S3_0_C12_C12_4, {}", in(reg) 0u64);  // ICC_CTLR_EL1
        asm!("isb");

        // 5. 使能 Group 1 中断
        asm!("msr S3_0_C12_C12_7, {}", in(reg) 1u64);  // ICC_IGRPEN1_EL1
        asm!("isb");
    }

    kprintln!("[GIC] CPU Interface initialized (SRE, PMR=0xFF, GRPEN1=1)");
}

// ==================== 公共接口 ====================

/// 使能一个 SPI 中断（INTID >= 32）
pub fn gic_enable_spi(intid: u32) {
    let base = gicd_base();
    let reg = base + GICD_ISENABLER + ((intid / 32) * 4) as usize;
    let bit = 1u32 << (intid % 32);
    write32(reg, bit);
    kprintln!("[GIC] SPI INTID {} enabled", intid);
}

/// 使能一个 PPI 中断（INTID 16-31）
pub fn gic_enable_ppi(cpu: usize, intid: u32) {
    let sgi_base = gicr_sgi_base(cpu);
    let bit = 1u32 << intid;
    write32(sgi_base + GICR_ISENABLER0, bit);
}

/// 完整 GIC 初始化
pub fn gic_init() {
    gic_dist_init();
    gic_redist_init(0);
    gic_cpu_interface_init();
}
