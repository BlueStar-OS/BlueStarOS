//! # AArch64 早期 MMU 页表 (Early MMU Page Table)
//!
//! ## 目的
//! 在堆分配器初始化之前，建立最小化的恒等映射页表，
//! 将内存属性从默认的 Device-nGnRnE 切换为 Normal Cacheable，
//! 使 exclusive/atomic 指令（LDXR/STXR）能正常工作。
//! 这是 lazy_static、spin::Once 等依赖原子操作的基础设施的前提。
//!
//! ## 页表结构 (2MB Block Descriptor)
//!
//! ```text
//!   TTBR0_EL1
//!       │
//!       ▼
//!   ┌─────────┐
//!   │ L1 Table│  512 项，每项覆盖 1GB
//!   │ (4KB)   │
//!   └────┬────┘
//!        │ L1[0] → Table Descriptor (bits[1:0]=0b11)
//!        ▼
//!   ┌─────────┐
//!   │ L2 Table│  512 项，每项覆盖 2MB
//!   │ (4KB)   │  使用 Block Descriptor (bits[1:0]=0b01)
//!   └─────────┘
//! ```
//!
//! ## L2 Block Descriptor 格式 (2MB 大页)
//!
//! ```text
//!   63  54 53 52 51 50    47          21 20  12 11 10 9:8 7:6 5 4:2 1 0
//!   ┌────┬──┬──┬──┬───────────────────┬──────┬──┬──┬───┬───┬──┬───┬───┐
//!   │UXN │PXN│  │DBM│  Output Address   │ SBZ  │nG│AF│SH │AP │NS│Idx│0 1│
//!   └────┴──┴──┴──┴───────────────────┴──────┴──┴──┴───┴───┴──┴───┴───┘
//!   注意: bits[1:0]=0b01 是 Block Descriptor，不是 Table/Page 的 0b11
//! ```
//!
//! ## 映射策略 (第一个 1GB 恒等映射)
//!
//! | 地址范围                  | L2 索引   | 属性    | 用途                    |
//! |--------------------------|----------|---------|------------------------|
//! | 0x00000000 - 0x01FFFFFF  | [0, 15]  | Normal  | 内核代码/数据/物理内存             |
//! | 0x02000000 - 0x07FFFFFF  | [16, 63] | Normal  | 内核代码/数据/物理内存   |
//! | 0x08000000 - 0x0FFFFFFF  | [64, 127]| Normal  | 内核代码/数据/物理内存      |
//! | 0x10000000 - 0x3FFFFFFF  | [128,511]| Normal  | 扩展物理内存            |
//!
//! ## 使用方式
//!
//! ```rust
//! clear_bss();                          // 1. 先清 BSS
//! unsafe { early_mmu_init(); }          // 2. 填充页表
//! let ttbr0 = unsafe { early_ttbr0() }; // 3. 获取页表基地址
//! // 4. 设置 MAIR/TCR/TTBR0/SCTLR 并开启 MMU (用户自行处理)
//! ```

// ============================================================================
// 2MB Block Descriptor 常量
// ============================================================================

use core::arch::asm;

use crate::{arch::memory::*, kprintln};

/// 2MB 块大小
const BLOCK_2MB: usize = 2 * 1024 * 1024;

/// Block Descriptor: bits[1:0] = 0b01
const DESC_BLOCK: u64 = 0b01;

/// Table Descriptor: bits[1:0] = 0b11
const DESC_TABLE: u64 = 0b11;

// --- 内存属性 (与 mod.rs 中 MAIR_EL1 配置一致) ---
// MAIR Attr0 = 0xFF → Normal WB
// MAIR Attr1 = 0x04 → Device nGnRE
const ATTR_NORMAL: u64 = 0 << 2; // AttrIndx = 0
const ATTR_DEVICE: u64 = 1 << 2; // AttrIndx = 1

// --- 通用属性位 ---
const AF: u64         = 1 << 10;     // Access Flag (必须为 1)
const SH_INNER: u64   = 0b11 << 8;   // Inner Shareable
const AP_RW_EL1: u64  = 0b00 << 6;   // AP[2:1]=00 → EL1:RW, EL0:无

// ============================================================================
// 组合属性: Normal 块 / Device 块
// ============================================================================

/// Normal Cacheable 2MB 块 (内核代码/数据/栈/堆)
/// AttrIndx=0(Normal WB) + AF + Inner Shareable + EL1 RW
const BLOCK_NORMAL: u64 = DESC_BLOCK | ATTR_NORMAL | AF | SH_INNER | AP_RW_EL1;

/// Device nGnRE 2MB 块 (MMIO 设备)
/// AttrIndx=1(Device) + AF + EL1 RW
/// Device 内存不设 SH (共享属性对 Device 无意义)
const BLOCK_DEVICE: u64 = DESC_BLOCK | ATTR_DEVICE | AF | AP_RW_EL1;

// ============================================================================
// 映射范围配置 (L2 索引边界)
// ============================================================================

/// 内核起始地址对应的 L2 索引: 0x02000000 / 2MB = 16
const KERNEL_START_IDX: usize = 16;

/// 内核区域结束 L2 索引 (不含): 0x08000000 / 2MB = 64
const KERNEL_END_IDX: usize = 64;

/// 高地址 MMIO 起始 L2 索引: 0x08000000 / 2MB = 64
const HI_MMIO_START_IDX: usize = 64;

/// 高地址 MMIO 结束 L2 索引 (不含): 0x10000000 / 2MB = 128
const HI_MMIO_END_IDX: usize = 128;

// ============================================================================
// 静态页表 (BSS 段，clear_bss 后为全零)
// ============================================================================

/// 4KB 对齐的页表结构
#[repr(C, align(4096))]
struct EarlyPageTable {
    entries: [u64; 512],
}

/// L1 页表 (每项覆盖 1GB)
static mut EARLY_L1: EarlyPageTable = EarlyPageTable { entries: [0; 512] };

/// L2 页表 — 覆盖第一个 1GB (0x00000000 - 0x3FFFFFFF)
static mut EARLY_L2_FIRST_GB: EarlyPageTable = EarlyPageTable { entries: [0; 512] };

/// L2 页表 — 覆盖第二个 1GB (0x40000000 - 0x5FFFFFFF)
static mut EARLY_L2_SECONED_GB: EarlyPageTable = EarlyPageTable { entries: [0; 512] };

/// L2 页表 — 覆盖第四个 1GB (0xC0000000 - 0xFFFFFFFF)
/// 包含 RK3588 UART2 (0xFEB50000)
static mut EARLY_L2_FOURTH_GB: EarlyPageTable = EarlyPageTable { entries: [0; 512] };

// ============================================================================
// 公开接口
// ============================================================================

/// 初始化早期页表 (恒等映射，2MB 大页)
///
/// 调用前提: clear_bss() 已执行完毕
///
/// 映射策略:
///   - [0, KERNEL_START_IDX):       Normal
///   - [KERNEL_START_IDX, KERNEL_END_IDX): Normal (内核 + 物理内存)
///   - [HI_MMIO_START_IDX, HI_MMIO_END_IDX): Normal
///   - [HI_MMIO_END_IDX, 512):     Normal (扩展内存)
pub unsafe fn early_mmu_init() {
    // 填充 L2 表: 512 个 2MB block descriptor
    for i in 0..512usize {
        let phys_addr = (i as u64) * (BLOCK_2MB as u64);

        let attr = if i < KERNEL_START_IDX {
            // 0x00000000 - 0x01FFFFFF: Device
            BLOCK_NORMAL
        } else if i < KERNEL_END_IDX {
            // 0x02000000 - 0x07FFFFFF: Normal
            BLOCK_NORMAL
        } else if i < HI_MMIO_END_IDX {
            // 0x08000000 - 0x0FFFFFFF: Device
            BLOCK_DEVICE
        } else {
            // 0x10000000 - 0x3FFFFFFF: Normal
            BLOCK_NORMAL
        };
        EARLY_L2_FIRST_GB.entries[i] = phys_addr | attr;

        EARLY_L2_SECONED_GB.entries[i] = 0x40000000 + (phys_addr | attr)
    }

    // L1[0] → L2 表 (Table Descriptor)
    let l2_phys = &EARLY_L2_FIRST_GB as *const _ as u64;
    let l2_phys_2 = &EARLY_L2_SECONED_GB as *const _ as u64;
    EARLY_L1.entries[0] = l2_phys   | DESC_TABLE;
    EARLY_L1.entries[1] = l2_phys_2 | DESC_TABLE;

    // 填充第四个 1GB 的 L2 表 (0xC0000000 - 0xFFFFFFFF)
    // 全部映射为 Device (UART、外设等)
    for i in 0..512usize {
        let phys_addr = (3u64 * 0x40000000) + (i as u64) * (BLOCK_2MB as u64);
        EARLY_L2_FOURTH_GB.entries[i] = phys_addr | BLOCK_DEVICE;
    }

    // L1[3] → 第四个 1GB 的 L2 表
    let l2_4th_phys = &EARLY_L2_FOURTH_GB as *const _ as u64;
    EARLY_L1.entries[3] = l2_4th_phys | DESC_TABLE;
}

/// 返回早期页表的物理基地址，用于写入 TTBR0_EL1
#[inline]
pub unsafe fn early_ttbr0() -> u64 {
    &EARLY_L1 as *const _ as u64
}




pub fn turn_early_mmu(){
let mut current_el: u64;
  unsafe { asm!("mrs {0}, CurrentEL", out(reg) current_el); }
  kprintln!("CurrentEL = {:#x}",current_el);  // bits[3:2] = EL
  // 0x4 = EL1, 0x8 = EL2, 0xC = EL3

    unsafe {
        asm!(
            "tlbi vmalle1",
            "dsb nsh"
        );
        kprintln!("Clear old table,clear old instruct");
        asm!(
            "msr mair_el1, {0}",in(reg) MAIR_EL1_VALUE
        );
        kprintln!("[MMU] Step 2: MAIR_EL1 = {:#x}", MAIR_EL1_VALUE);
        // Step 3: 配置 TCR_EL1
        core::arch::asm!(
            "msr tcr_el1, {0}",
            in(reg) TCR_EL1_VALUE
        );
        kprintln!("[MMU] Step 3: TCR_EL1 = {:#x}", TCR_EL1_VALUE);

        // Step 4: ISB 确保系统寄存器写入完成
        core::arch::asm!("isb");

        // Step 5: 设置 TTBR0_EL1（内核页表，低地址空间）
        // 当前设计: 内核运行在 TTBR0 管理的低地址空间 (0x40080000)
        // TTBR1 被 TCR.EPD1=1 禁用
        core::arch::asm!(
            "msr ttbr0_el1, {0}",
            in(reg) early_ttbr0()
        );
        kprintln!("[MMU] Step 5: TTBR0_EL1 = {:#x}", early_ttbr0());

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
        kprintln!("[MMU] Step 7: current SCTLR_EL1 = {:#x}", sctlr);

        sctlr |= SCTLR_ELX_M | SCTLR_ELX_C | SCTLR_ELX_I;
        kprintln!("[MMU] Step 7: Will write SCTLR_EL1 = {:#x} (M|C|I)", sctlr);

        core::arch::asm!(
            "msr sctlr_el1, {0}",
            "isb",
            in(reg) sctlr
        );

        // Step 8: 失效指令缓存
        // 来源: head.S __enable_mmu — ic iallu; dsb nsh; isb
        core::arch::asm!(
            "ic iallu",
            "dsb nsh",
            "isb"
        );

        kprintln!("[MMU]: Aarch64 Early mmu turn on!")
    }
    
}