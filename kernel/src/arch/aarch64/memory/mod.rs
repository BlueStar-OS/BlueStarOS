//! # AArch64 MMU 初始化与页表激活模块
//!
//! ## 概述
//! 本模块负责配置 AArch64 的 MMU 相关系统寄存器并启用虚拟地址翻译。
//! 严格参考 Linux 5.4.29 的启动路径:
//!   - `arch/arm64/mm/proc.S` → `__cpu_setup` (配置 MAIR/TCR)
//!   - `arch/arm64/kernel/head.S` → `__enable_mmu` (写入 TTBR, 使能 SCTLR.M)
//!
//! ## MMU 启用的完整流程
//!
//! ```text
//!   ┌─────────────────────────────────────────────────────────────┐
//!   │ Step 1: tlbi vmalle1 + dsb nsh                             │
//!   │         失效所有 TLB 条目，确保旧的翻译不会干扰新页表       │
//!   ├─────────────────────────────────────────────────────────────┤
//!   │ Step 2: msr mair_el1, MAIR_VALUE                           │
//!   │         配置内存属性: Attr0=Normal WB, Attr1=Device nGnRE   │
//!   ├─────────────────────────────────────────────────────────────┤
//!   │ Step 3: msr tcr_el1, TCR_VALUE                             │
//!   │         配置翻译控制: 39-bit VA, 4KB 粒度, 40-bit PA        │
//!   ├─────────────────────────────────────────────────────────────┤
//!   │ Step 4: isb                                                │
//!   │         指令同步屏障，确保 MAIR/TCR 写入生效                 │
//!   ├─────────────────────────────────────────────────────────────┤
//!   │ Step 5: msr ttbr0_el1, 页表基地址                           │
//!   │         设置 L1 页表的物理地址                               │
//!   ├─────────────────────────────────────────────────────────────┤
//!   │ Step 6: isb                                                │
//!   │         确保 TTBR 写入生效                                  │
//!   ├─────────────────────────────────────────────────────────────┤
//!   │ Step 7: msr sctlr_el1, (M|C|I)                            │
//!   │         使能 MMU(M) + 数据缓存(C) + 指令缓存(I)            │
//!   │         从这一刻起，所有内存访问都经过 MMU 翻译!             │
//!   ├─────────────────────────────────────────────────────────────┤
//!   │ Step 8: isb                                                │
//!   │         确保 SCTLR 写入生效                                 │
//!   ├─────────────────────────────────────────────────────────────┤
//!   │ Step 9: ic iallu + dsb nsh + isb                           │
//!   │         失效整个指令缓存，确保取指使用新的翻译               │
//!   └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## 关键系统寄存器
//!
//! | 寄存器      | 作用                                                    |
//! |------------|--------------------------------------------------------|
//! | MAIR_EL1   | 定义 8 种内存属性 (Normal/Device/NC 等)，PTE 的 AttrIndx 索引它 |
//! | TCR_EL1    | 翻译控制: VA 大小、粒度、缓存策略、物理地址宽度            |
//! | TTBR0_EL1  | 低地址空间页表基地址 (我们的内核页表)                      |
//! | TTBR1_EL1  | 高地址空间页表基地址 (当前被 EPD1=1 禁用)                  |
//! | SCTLR_EL1  | 系统控制: M=MMU使能, C=D-cache使能, I=I-cache使能         |

pub mod address;
pub mod eaylymmu;

pub use address::*;
pub use eaylymmu::{early_mmu_init, early_ttbr0};

use crate::memory::MapSet;

// =============================================================================
// AArch64 系统寄存器常量
// 来源: Linux arch/arm64/include/asm/pgtable-hwdef.h
// 来源: Linux arch/arm64/include/asm/sysreg.h
// 来源: Linux arch/arm64/mm/proc.S __cpu_setup
// =============================================================================

// --- MAIR_EL1 (Memory Attribute Indirection Register) ---
//
// MAIR_EL1 定义了 8 个内存属性槽 (Attr0~Attr7)，每个 8 bits。
// PTE 中的 AttrIndx[4:2] 字段索引这些槽，决定该页的内存类型。
//
// 来源: proc.S __cpu_setup 中的 MAIR 配置
//
// 我们只使用 2 个槽:
//   Attr0 = 0xFF → Normal Memory, Inner/Outer Write-Back, Read/Write Allocate
//                   编码: Outer[7:4]=0xF (WB RWA), Inner[3:0]=0xF (WB RWA)
//                   用于: 代码、数据、栈、堆、页表
//
//   Attr1 = 0x04 → Device Memory, nGnRE
//                   编码: 0b0000_0100 (Device-nGnRE)
//                   n = non-Gathering:  不合并多次访问
//                   n = non-Reordering: 不重排访问顺序
//                   E = Early Write Acknowledgement: 允许写缓冲
//                   用于: UART, VirtIO, GIC 等 MMIO 设备
const MAIR_ATTR0_NORMAL_WB: u64 = 0xFF; // Attr0[7:0]
const MAIR_ATTR1_DEVICE_NGNRE: u64 = 0x04 << 8; // Attr1[15:8]
const MAIR_EL1_VALUE: u64 = MAIR_ATTR0_NORMAL_WB | MAIR_ATTR1_DEVICE_NGNRE;
// = 0x04FF

// --- TCR_EL1 (Translation Control Register) ---
//
// TCR_EL1 控制地址翻译的所有参数。
// 来源: pgtable-hwdef.h TCR_xxx 定义
// 来源: proc.S __cpu_setup 中的 TCR 组合
//
// 重要修复历史: 原始魔法数 0x0000000286003499 有 3 个致命 bug:
//   1. EPD0=1 (bit[7]=1) — 禁用了 TTBR0 翻译! 内核在 TTBR0 地址空间，
//      MMU 启用后第一条指令就触发 Translation Fault
//   2. T1SZ=0 (bits[21:16]=0) — 应为 25 (39-bit VA)
//   3. IRGN0=0 (bits[9:8]=0) — 页表遍历非缓存，严重影响性能
// 修复后正确值: 0x0000000280993519
//
// TCR_EL1 位域布局:
//   bits[5:0]   T0SZ  = 25  → TTBR0 地址空间 = 2^(64-25) = 2^39 = 512GB
//   bits[9:8]   IRGN0 = 01  → TTBR0 页表遍历 Inner Write-Back Write-Allocate
//   bits[11:10] ORGN0 = 01  → TTBR0 页表遍历 Outer Write-Back Write-Allocate
//   bits[13:12] SH0   = 11  → TTBR0 页表遍历 Inner Shareable
//   bits[15:14] TG0   = 00  → TTBR0 粒度 4KB
//   bits[21:16] T1SZ  = 25  → TTBR1 地址空间 = 2^39
//   bit[23]     EPD1  = 0   → 启用 TTBR1 翻译 (内核栈在高地址空间)
//   bits[31:30] TG1   = 10  → TTBR1 粒度 4KB
//   bits[34:32] IPS   = 010 → 物理地址 40-bit (1TB)

// T0SZ: TTBR0 管理的虚拟地址空间大小 = 2^(64 - T0SZ)
// T0SZ=25 → 2^39 = 512GB，即 39-bit 虚拟地址
const TCR_T0SZ_OFFSET: u32 = 0;
const TCR_T0SZ_39BIT: u64 = 25 << TCR_T0SZ_OFFSET as u64; // bits[5:0] = 25

// T1SZ: TTBR1 管理的虚拟地址空间大小 (虽然 EPD1=1 禁用了 TTBR1，但仍需正确设置)
const TCR_T1SZ_OFFSET: u32 = 16;
const TCR_T1SZ_39BIT: u64 = 25 << TCR_T1SZ_OFFSET as u64; // bits[21:16] = 25

// EPD0=0 (默认): 启用 TTBR0 翻译 — 内核运行在 TTBR0 管理的低地址空间
// EPD1=1: 禁用 TTBR1 翻译 — 当前不使用高地址空间
// 如果误设 EPD0=1，MMU 启用后第一条指令就会触发 Translation Fault!
const TCR_EPD1: u64 = 1 << 23;
const TCR_EPD0: u64 = 0 << 23;

// IRGN0/ORGN0: TTBR0 页表遍历 (page table walk) 的缓存策略
// 01 = Write-Back Write-Allocate — 页表遍历经过缓存，大幅提升 TLB miss 时的性能
// 如果设为 00 (Non-cacheable)，每次 TLB miss 都要从内存读页表，非常慢
const TCR_IRGN0_WBWA: u64 = 1 << 8; // Inner cacheability
const TCR_ORGN0_WBWA: u64 = 1 << 10; // Outer cacheability

// SH0: TTBR0 页表遍历的共享属性
// 11 = Inner Shareable — 多核系统中，页表遍历结果在所有核心间一致
const TCR_SH0_INNER: u64 = 3 << 12;

// TG0: TTBR0 的翻译粒度 (Granule)
// 00 = 4KB — 最常用，每级页表 512 项 (9 bits 索引)
const TCR_TG0_4K: u64 = 0 << 14;

// TG1: TTBR1 的翻译粒度
// 10 = 4KB
const TCR_TG1_4K: u64 = 2 << 30;

// IRGN1/ORGN1: TTBR1 页表遍历的缓存策略 (与 TTBR0 对称)
// 01 = Write-Back Write-Allocate — 页表遍历经过缓存
// 如果设为 00 (Non-cacheable)，内核写入的 PTE 还在 D-cache 中未刷出，
// MMU 做 TTBR1 页表遍历时绕过缓存直接读 DRAM，看到全零 → Translation Fault!
const TCR_IRGN1_WBWA: u64 = 1 << 24; // Inner cacheability
const TCR_ORGN1_WBWA: u64 = 1 << 26; // Outer cacheability

// SH1: TTBR1 页表遍历的共享属性
// 11 = Inner Shareable — 多核系统中页表遍历结果在所有核心间一致
const TCR_SH1_INNER: u64 = 3 << 28;

// IPS: 中间物理地址大小 (Intermediate Physical Address Size)
// 010 = 40-bit → 最大支持 1TB 物理内存
const TCR_IPS_40BIT: u64 = 2 << 32;

// 组合 TCR 值
// T0SZ=25, IRGN0=WB, ORGN0=WB, SH0=ISH, TG0=4K,
// T1SZ=25, EPD1=0, IRGN1=WB, ORGN1=WB, SH1=ISH, TG1=4K, IPS=40bit
const TCR_EL1_VALUE: u64 = TCR_T0SZ_39BIT
    | TCR_T1SZ_39BIT
    | TCR_IRGN0_WBWA
    | TCR_ORGN0_WBWA
    | TCR_SH0_INNER
    | TCR_TG0_4K
    | TCR_EPD0
    | TCR_IRGN1_WBWA
    | TCR_ORGN1_WBWA
    | TCR_SH1_INNER
    | TCR_TG1_4K
    | TCR_IPS_40BIT;

// --- SCTLR_EL1 (System Control Register) ---
//
// SCTLR_EL1 控制 MMU、缓存等核心功能的开关。
// 来源: sysreg.h SCTLR_ELx_xxx
//
// 我们在 active_memset() 中用 读-改-写 方式设置这 3 个位:
//   M (bit[0]):  MMU 使能 — 从此刻起所有内存访问都经过页表翻译
//   C (bit[2]):  数据缓存使能 — 允许 D-cache 缓存数据访问
//   I (bit[12]): 指令缓存使能 — 允许 I-cache 缓存指令取指
//
// 注意: 必须在 MAIR/TCR/TTBR 都配置好之后才能设置 M=1!
//       否则 MMU 会用未初始化的寄存器进行翻译，导致未定义行为。
const SCTLR_ELX_M: u64 = 1 << 0; // MMU 使能
const SCTLR_ELX_C: u64 = 1 << 2; // 数据缓存使能
const SCTLR_ELX_I: u64 = 1 << 12; // 指令缓存使能

/// 激活内存映射集：配置 MMU 系统寄存器并切换页表
///
/// 这是 AArch64 MMU 启用的核心函数。
/// 严格参考 Linux 启动路径:
///   - `proc.S __cpu_setup` (配置 MAIR/TCR)
///   - `head.S __enable_mmu` (写入 TTBR, 使能 SCTLR.M)
///
/// 执行顺序 (不可更改，否则会导致 MMU 行为未定义):
///
///   Step 1: tlbi vmalle1 + dsb nsh
///           失效所有 TLB 条目。必须在写入 TTBR 之前执行，
///           否则旧的 TLB 缓存可能指向错误的物理地址。
///           来源: proc.S:408
///
///   Step 2: msr mair_el1, MAIR_VALUE
///           配置内存属性索引表。PTE 中的 AttrIndx 字段引用这里的定义。
///           来源: proc.S:436
///
///   Step 3: msr tcr_el1, TCR_VALUE
///           配置翻译控制参数: VA 大小、粒度、缓存策略、PA 宽度。
///           来源: proc.S:476
///
///   Step 4: isb
///           指令同步屏障。确保 MAIR/TCR 的写入在后续指令执行前完成。
///           没有 ISB，CPU 可能用旧的 MAIR/TCR 值进行翻译。
///
///   Step 5: msr ttbr0_el1, 页表基地址
///           设置 L1 页表的物理地址。MMU 启用后从这里开始翻译。
///           来源: head.S:784
///
///   Step 6: isb
///           确保 TTBR 写入生效。
///
///   Step 7: msr sctlr_el1, (M|C|I)
///           使能 MMU(M) + 数据缓存(C) + 指令缓存(I)。
///           从这一刻起，所有内存访问都经过 MMU 翻译!
///           来源: head.S:788
///
///   Step 8: isb (在 Step 7 的 asm 块中)
///           确保 SCTLR 写入生效，CPU 立即开始使用 MMU。
///
///   Step 9: ic iallu + dsb nsh + isb
///           失效整个指令缓存。因为 MMU 启用前后指令的物理地址映射可能不同，
///           必须清除 I-cache 中可能缓存的旧指令。
///           来源: head.S:795-797
#[no_mangle]
pub fn active_memset(memset: &MapSet) {
    use log::debug;

    let ttbr0_value = memset.table.satp_token();
    debug!("[MMU] 页表根物理地址: {:#x}", ttbr0_value);

    unsafe {
        // Step 1: 失效所有 TLB 条目（必须在设置 TTBR 之前）
        // 来源: proc.S __cpu_setup 第一条指令
        core::arch::asm!("tlbi vmalle1", "dsb nsh",);
        debug!("[MMU] Step 1: TLB 已失效 (tlbi vmalle1 + dsb nsh)");

        // Step 2: 配置 MAIR_EL1
        // Attr0=0xFF (Normal WB), Attr1=0x04 (Device nGnRE)
        core::arch::asm!(
            "msr mair_el1, {0}",
            in(reg) MAIR_EL1_VALUE
        );
        debug!("[MMU] Step 2: MAIR_EL1 = {:#x}", MAIR_EL1_VALUE);

        // Step 3: 配置 TCR_EL1
        core::arch::asm!(
            "msr tcr_el1, {0}",
            in(reg) TCR_EL1_VALUE
        );
        debug!("[MMU] Step 3: TCR_EL1 = {:#x}", TCR_EL1_VALUE);

        // Step 4: ISB 确保系统寄存器写入完成
        core::arch::asm!("isb");

        // Step 5: 设置 TTBR0_EL1（内核页表，低地址空间）
        // 当前设计: 内核运行在 TTBR0 管理的低地址空间 (0x40080000)
        // TTBR1 被 TCR.EPD1=1 禁用
        core::arch::asm!(
            "msr ttbr0_el1, {0}",
            in(reg) ttbr0_value
        );
        debug!("[MMU] Step 5: TTBR0_EL1 = {:#x}", ttbr0_value);

        // 和ttbr0用同一张表
        core::arch::asm!(
            "msr ttbr1_el1, {0}",
            in(reg) ttbr0_value
        );
        debug!("[MMU] Step 6: TTBR1_EL1 = {:#x}", ttbr0_value);

        // Step 6: ISB 确保 TTBR 写入完成
        core::arch::asm!("isb");

        // Step 7: 使能 MMU — 读-改-写 SCTLR_EL1
        // 来源: head.S __enable_mmu — msr sctlr_el1, x0
        // 来源: sysreg.h SCTLR_EL1_SET 包含 M|C|I
        let mut sctlr: u64;
        core::arch::asm!(
            "mrs {0}, sctlr_el1",
            out(reg) sctlr
        );
        debug!("[MMU] Step 7: 当前 SCTLR_EL1 = {:#x}", sctlr);

        sctlr |= SCTLR_ELX_M | SCTLR_ELX_C | SCTLR_ELX_I;
        debug!("[MMU] Step 7: 即将写入 SCTLR_EL1 = {:#x} (M|C|I)", sctlr);

        core::arch::asm!(
            "msr sctlr_el1, {0}",
            "isb",
            in(reg) sctlr
        );

        // Step 8: 失效指令缓存
        // 来源: head.S __enable_mmu — ic iallu; dsb nsh; isb
        core::arch::asm!("ic iallu", "dsb nsh", "isb");

        debug!("[MMU] MMU Turned! I-cache already into invalid");

        // 验证 MMU 状态
        let mut sctlr_verify: u64;
        core::arch::asm!(
            "mrs {0}, sctlr_el1",
            out(reg) sctlr_verify
        );
        debug!(
            "[MMU] Verify SCTLR_EL1 = {:#x}, M={}, C={}, I={}",
            sctlr_verify,
            (sctlr_verify >> 0) & 1,
            (sctlr_verify >> 2) & 1,
            (sctlr_verify >> 12) & 1
        );
    }
}
