//! # AArch64 页表项 (PTE) 编码与解码模块
//!
//! ## 概述
//! 本模块实现 ARMv8-A VMSAv8-64 页表格式的编码/解码。
//! 采用 4KB 粒度 (Granule)、39-bit 虚拟地址空间、3 级页表翻译 (L1→L2→L3)。
//!
//! ## AArch64 地址翻译流程 (4KB Granule, 39-bit VA)
//!
//! ```text
//!   虚拟地址 (39-bit):
//!   ┌──────────┬──────────┬──────────┬──────────┐
//!   │ L1 index │ L2 index │ L3 index │  offset  │
//!   │ [38:30]  │ [29:21]  │ [20:12]  │  [11:0]  │
//!   │  9 bits  │  9 bits  │  9 bits  │ 12 bits  │
//!   └────┬─────┴────┬─────┴────┬─────┴──────────┘
//!        │          │          │
//!        ▼          ▼          ▼
//!   ┌─────────┐ ┌─────────┐ ┌─────────┐
//!   │ L1 Table│→│ L2 Table│→│ L3 Table│→ 物理页帧
//!   │ 512项   │ │ 512项   │ │ 512项   │
//!   └─────────┘ └─────────┘ └─────────┘
//!        ↑
//!     TTBR0_EL1 (页表基地址寄存器)
//! ```
//!
//! ## 描述符格式
//!
//! ### Table Descriptor (L1/L2 中间级，指向下一级页表)
//! ```text
//!   63                              47  12  1 0
//!   ┌──────────────────────────────┬──────┬─┬─┐
//!   │         忽略/保留            │ 地址 │1│1│  bits[1:0]=0b11
//!   └──────────────────────────────┴──────┴─┴─┘
//!   注意: Table descriptor 不包含 AP/SH/nG/AttrIdx 等页级属性!
//!         只需要 bits[1:0]=0b11 + 下一级页表的物理地址
//! ```
//!
//! ### Page Descriptor (L3 最终级，映射到物理页帧)
//! ```text
//!   63  54 53 51 50  47          12 11 10 9:8 7:6 5 4:2 1 0
//!   ┌────┬──┬──┬───┬──────────────┬──┬──┬───┬───┬──┬───┬───┐
//!   │UXN │PXN│DBM│SW│  物理地址    │nG│AF│SH │AP │NS│Idx│1 1│
//!   └────┴──┴──┴───┴──────────────┴──┴──┴───┴───┴──┴───┴───┘
//! ```
//!
//! ## 各属性位详解
//!
//! | 位域     | 名称     | 含义                                                  |
//! |---------|---------|------------------------------------------------------|
//! | [1:0]   | Type    | 0b11=有效描述符, 0b00=无效                              |
//! | [4:2]   | AttrIndx| 指向 MAIR_EL1 的属性槽: 0=Normal WB, 1=Device nGnRE   |
//! | [6]     | AP[1]   | 0=仅EL1可访问, 1=EL0也可访问 (用户态)                    |
//! | [7]     | AP[2]   | 0=读写, 1=只读                                        |
//! | [9:8]   | SH      | 共享属性: 0b11=Inner Shareable (多核一致性)              |
//! | [10]    | AF      | Access Flag: 必须=1, 否则触发 Access Flag Fault        |
//! | [11]    | nG      | non-Global: 1=进程私有(用ASID), 0=全局(内核)            |
//! | [47:12] | 地址     | 物理页帧地址 (4KB 对齐, 36 位)                          |
//! | [51]    | DBM     | Dirty Bit Management: 硬件脏页追踪                     |
//! | [53]    | PXN     | Privileged Execute-Never: 1=内核不可执行               |
//! | [54]    | UXN     | User Execute-Never: 1=用户态不可执行                   |
//!
//! ## PTEFlags → AArch64 硬件位 翻译规则
//!
//! | PTEFlags | AArch64 硬件位                    | 说明                          |
//! |----------|----------------------------------|-------------------------------|
//! | V        | bits[1:0]=0b11                   | 有效描述符                      |
//! | R        | (默认可读，AP[2]控制只读)           | AArch64 没有单独的 R 位         |
//! | W        | AP[2]=0 (不设 PTE_RDONLY)         | 可写                           |
//! | X        | PXN=0, UXN=0/1                   | 可执行 (根据 U 决定 UXN)        |
//! | U        | AP[1]=1 (PTE_USER)               | 用户态可访问                    |
//! | G        | nG=0                             | 全局映射 (取反逻辑)             |
//! | A        | AF=1 (但 AF 始终设为 1)           | 已访问                         |
//! | D        | DBM=1                            | 脏页                           |
//! | DEV      | AttrIndx=1 (Device nGnRE)        | 设备内存                       |

use bitflags::bitflags;
use crate::PAGE_SIZE_BITS;
use crate::PAGE_SIZE;
use crate::memory::FramTracker;
use alloc::vec::Vec;
use alloc::vec;
use core::sync::atomic::compiler_fence;
use crate::error;
use core::sync::atomic::Ordering;
use crate::memory::alloc_frame;
use log::debug;

// =============================================================================
// AArch64 页表硬件常量
// 来源: Linux arch/arm64/include/asm/pgtable-hwdef.h
// 来源: ARM Architecture Reference Manual, Chapter D5 "The VMSAv8-64 Translation Table Format"
// =============================================================================

// --- 描述符类型 bits[1:0] ---
//
// AArch64 页表描述符的最低 2 位决定描述符类型:
//   0b00 = Invalid (无效，MMU 忽略此条目，访问触发 Translation Fault)
//   0b01 = Block descriptor (L1/L2 大页映射，直接映射到物理块)
//   0b11 = Table descriptor (L1/L2 指向下一级页表) 或 Page descriptor (L3 映射到 4KB 页)
//
// 关键区别 — Table descriptor vs Page descriptor:
//   虽然 bits[1:0] 都是 0b11，但语义完全不同:
//   - Table descriptor (L1/L2): 只包含下一级页表的物理地址，不含任何内存属性
//   - Page descriptor (L3):     包含物理页地址 + 完整的内存属性 (AP/SH/AF/nG/AttrIdx/PXN/UXN)
//   如果在 Table descriptor 中错误地注入了页属性，会导致 MMU 行为未定义!
//   (这是之前 find_or_create_pte_vpn 的 bug 根源)
//
// Table descriptor (L0/L1/L2 指向下一级页表): bits[1:0] = 0b11
// Page descriptor  (L3 最终页映射):           bits[1:0] = 0b11
// Block descriptor (L1/L2 大页映射):          bits[1:0] = 0b01
// Invalid:                                    bits[1:0] = 0b00
const DESC_VALID: usize     = 1 << 0;  // PTE_VALID
const DESC_TABLE_BIT: usize = 1 << 1;  // PTE_TABLE_BIT
const DESC_TYPE_TABLE: usize = DESC_VALID | DESC_TABLE_BIT; // 0b11
const DESC_TYPE_PAGE: usize  = DESC_VALID | DESC_TABLE_BIT; // 0b11 (L3)

// --- 内存属性索引 AttrIndx[4:2] ---
//
// AttrIndx 是一个 3-bit 索引，指向 MAIR_EL1 寄存器中的 8 个属性槽 (Attr0~Attr7)。
// 每个槽定义了一种内存类型 (Normal/Device/Non-cacheable 等)。
//
// 我们的 MAIR_EL1 布局 (在 mod.rs 中配置):
//   Attr0 = 0xFF → Normal Memory, Inner/Outer Write-Back, Read/Write Allocate
//                   用于: 代码段、数据段、栈、堆、页表自身
//   Attr1 = 0x04 → Device Memory, nGnRE (non-Gathering, non-Reordering, Early Write Ack)
//                   用于: MMIO 设备 (UART, VirtIO, GIC 等)
//
// 为什么设备内存必须用 Device 属性?
//   Normal 内存允许 CPU 合并/重排/推测读取，这对 MMIO 寄存器是灾难性的。
//   Device nGnRE 保证: 每次访问都到达设备，不合并，不重排，不推测。
// 指向 MAIR_EL1 中的 Attr 槽位
// 我们的 MAIR 布局: Attr0=0xFF(Normal WB), Attr1=0x04(Device nGnRE)
const PTE_ATTRINDX_SHIFT: usize  = 2;
const PTE_ATTRINDX_MASK: usize   = 0b111 << PTE_ATTRINDX_SHIFT;
const PTE_ATTRINDX_NORMAL: usize = 0 << PTE_ATTRINDX_SHIFT; // AttrIdx=0: Normal
const PTE_ATTRINDX_DEVICE: usize = 1 << PTE_ATTRINDX_SHIFT; // AttrIdx=1: Device

// --- 访问权限 AP[2:1] bits[7:6] ---
//
// AP (Access Permission) 控制 EL1 (内核) 和 EL0 (用户) 的读写权限:
//
//   AP[2:1] │ EL1 (内核) │ EL0 (用户)
//   ────────┼────────────┼───────────
//    0b00   │    RW      │   无权限     ← 内核读写，用户不可访问 (默认)
//    0b01   │    RW      │    RW       ← 内核和用户都可读写
//    0b10   │    RO      │   无权限     ← 内核只读，用户不可访问
//    0b11   │    RO      │    RO       ← 内核和用户都只读
//
// AP[1] (bit[6]) = PTE_USER:  置 1 允许 EL0 访问
// AP[2] (bit[7]) = PTE_RDONLY: 置 1 变为只读
// AP[2:1]=00 → EL1:RW, EL0:无
// AP[2:1]=01 → EL1:RW, EL0:RW
// AP[2:1]=10 → EL1:RO, EL0:无
// AP[2:1]=11 → EL1:RO, EL0:RO
const PTE_USER: usize   = 1 << 6;  // AP[1] — EL0 可访问
const PTE_RDONLY: usize  = 1 << 7;  // AP[2] — 只读

// --- 共享属性 SH[9:8] ---
//
// SH (Shareability) 控制多核间的缓存一致性:
//   0b00 = Non-shareable (单核，不保证一致性)
//   0b10 = Outer Shareable (外部共享域)
//   0b11 = Inner Shareable (内部共享域，最常用)
//
// 对于 SMP 系统，Normal 内存通常设为 Inner Shareable，
// 确保所有 CPU 核心看到一致的内存视图。
const PTE_SHARED: usize = 0b11 << 8; // Inner Shareable

// --- Access Flag bit[10] ---
//
// AF (Access Flag) 必须为 1，否则 CPU 首次访问该页时触发 Access Flag Fault。
// 如果操作系统没有实现 Access Flag Fault 处理程序，就会直接崩溃。
//
// Linux 的做法: 在所有有效映射中始终设置 AF=1 (参见 PAGE_KERNEL 等宏)。
// 我们也采用同样策略: flags_to_aarch64() 中无条件设置 PTE_AF。
// 必须为 1，否则硬件触发 Access Flag Fault
// Linux 在所有有效映射中始终设置此位
const PTE_AF: usize = 1 << 10;

// --- non-Global bit[11] ---
//
// nG (non-Global) 控制 TLB 条目是否与 ASID (Address Space ID) 关联:
//   nG=0 → 全局映射，所有进程共享 (内核空间)，TLB 条目不带 ASID 标签
//   nG=1 → 进程私有映射，TLB 条目带 ASID 标签 (用户空间)
//
// 进程切换时，全局映射的 TLB 条目不需要刷新，提高性能。
// nG=1: 该映射与 ASID 关联（进程私有）
// nG=0: 全局映射（内核空间）
const PTE_NG: usize = 1 << 11;

// --- 物理地址掩码 bits[47:12] ---
//
// 4KB 粒度下，物理地址的 bits[47:12] 存储在描述符中 (共 36 位)。
// bits[11:0] 是页内偏移，不存储在 PTE 中。
// 掩码 = ((1 << 36) - 1) << 12 = 0x0000_FFFF_FFFF_F000
const PTE_ADDR_MASK: usize = ((1usize << 36) - 1) << 12;

// --- Dirty Bit Management bit[51] ---
//
// DBM (Dirty Bit Management) 启用硬件脏页追踪。
// 当 DBM=1 且页被写入时，硬件自动清除 AP[2] (变为可写)，
// 操作系统可以通过检查 AP[2] 判断页是否被修改过。
const PTE_DBM: usize = 1 << 51;

// --- Execute-Never 位 ---
//
// PXN (Privileged Execute-Never) bit[53]:
//   PXN=1 → 内核 (EL1) 不可从此页执行代码
//   PXN=0 → 内核可执行
//
// UXN (User Execute-Never) bit[54]:
//   UXN=1 → 用户态 (EL0) 不可从此页执行代码
//   UXN=0 → 用户态可执行
//
// 安全原则 (W^X):
//   - 数据页:   PXN=1, UXN=1 (可写不可执行)
//   - 内核代码: PXN=0, UXN=1 (内核可执行，用户不可)
//   - 用户代码: PXN=1, UXN=0 (用户可执行，内核不可 — 防止 ret2usr 攻击)
const PTE_PXN: usize = 1 << 53; // Privileged Execute-Never
const PTE_UXN: usize = 1 << 54; // User Execute-Never

// =============================================================================
// 平台无关页表标志 (PTEFlags)
// =============================================================================
//
// 这些标志是操作系统内部的抽象表示，不直接写入硬件 PTE。
// 在创建 PageTableEntry 时，通过 flags_to_aarch64() 翻译为 AArch64 硬件位。
//
// 位布局与 RISC-V SV39 的 PTE 标志兼容 (bits 0-7)，
// bit 8 (DEV) 使用 RISC-V 的 RSW 保留位，硬件忽略。
bitflags! {
    /// 平台无关的页表标志位（与 MapAreaFlags 一一对应）
    /// 这些标志在 flags_to_aarch64() 中被翻译为 AArch64 硬件位
    #[derive(Debug, Clone, Copy)]
    pub struct PTEFlags: usize {
        /// 有效位
        const V = 1 << 0;
        /// 可读
        const R = 1 << 1;
        /// 可写
        const W = 1 << 2;
        /// 可执行
        const X = 1 << 3;
        /// 用户态可访问
        const U = 1 << 4;
        /// 全局映射（AArch64: nG 位取反）
        const G = 1 << 5;
        /// 已访问（AArch64: AF 位）
        const A = 1 << 6;
        /// 脏页（AArch64: DBM 位）
        const D = 1 << 7;
        /// 设备内存 — 使用 AttrIndx=1 (Device nGnRE)
        const DEV = 1 << 8;
    }
}

// =============================================================================
// 地址类型定义
// =============================================================================
//
// 页号 = 地址 >> 12 (右移 PAGE_SIZE_BITS 位，去掉页内偏移)
// 地址 = 页号 << 12 (左移 PAGE_SIZE_BITS 位，恢复完整地址)

/// 虚拟页号 (Virtual Page Number)
/// 39-bit VA 中去掉 12-bit 页内偏移后的 27-bit 值
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct VirNumber(pub usize);

/// 物理页号 (Physical Page Number)
/// 物理地址去掉 12-bit 页内偏移后的值
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PhysiNumber(pub usize);

/// 虚拟地址 (完整 39-bit 虚拟地址)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct VirAddr(pub usize);

/// 物理地址 (完整物理地址)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PhysiAddr(pub usize);

impl From<PhysiNumber> for PhysiAddr {
    fn from(value: PhysiNumber) -> Self {
        PhysiAddr(value.0 << PAGE_SIZE_BITS)
    }
}

impl From<VirNumber> for VirAddr {
    fn from(value: VirNumber) -> Self {
        VirAddr(value.0 << PAGE_SIZE_BITS)
    }
}

impl From<PhysiAddr> for PhysiNumber {
    fn from(value: PhysiAddr) -> Self {
        PhysiNumber(value.0 >> PAGE_SIZE_BITS)
    }
}

impl From<VirAddr> for VirNumber {
    fn from(value: VirAddr) -> Self {
        VirNumber(value.0 >> PAGE_SIZE_BITS)
    }
}

impl VirNumber {
    /// 将虚拟页号分解为 3 级页表索引
    /// AArch64 39-bit VA, 4KB granule, 3 级翻译 (L1, L2, L3)
    /// L1 索引: bits[38:30] — 覆盖 1GB
    /// L2 索引: bits[29:21] — 覆盖 2MB
    /// L3 索引: bits[20:12] — 覆盖 4KB
    pub fn index(&self) -> [usize; 3] {
        let vpn = self.0;
        let mut idx: [usize; 3] = [0; 3];
        idx[0] = (vpn >> 18) & 0x1FF;  // L1 索引
        idx[1] = (vpn >> 9) & 0x1FF;   // L2 索引
        idx[2] = vpn & 0x1FF;           // L3 索引
        idx
    }

    pub fn step(&mut self) -> Self {
        self.0 += 1;
        self.clone()
    }
}

impl VirAddr {
    pub fn floor_up(&self) -> VirNumber {
        let addr = self.0;
        VirNumber((addr + PAGE_SIZE - 1) / PAGE_SIZE)
    }

    pub fn floor_down(&self) -> VirNumber {
        let addr = self.0;
        VirNumber(addr / PAGE_SIZE)
    }

    pub fn offset(&self) -> usize {
        self.0 % PAGE_SIZE
    }

    pub fn strict_into_virnum(&self) -> VirNumber {
        if self.0 % PAGE_SIZE != 0 {
            panic!("strict_into_virnum Failed!!");
        }
        VirNumber(self.0 / PAGE_SIZE)
    }
}

impl PhysiAddr {
    pub fn floor_up(&self) -> PhysiNumber {
        let addr = self.0;
        PhysiNumber((addr + PAGE_SIZE - 1) / PAGE_SIZE)
    }

    pub fn floor_down(&self) -> PhysiNumber {
        let addr = self.0;
        PhysiNumber(addr / PAGE_SIZE)
    }

    pub fn offset(&self) -> usize {
        self.0 & (PAGE_SIZE - 1)
    }
}

// =============================================================================
// 页表项 (PageTableEntry)
// =============================================================================
//
// 一个 64-bit 的 AArch64 页表描述符。
// 根据所在层级不同，可以是:
//   - Table descriptor (L1/L2): 只含下一级页表地址，用 new_table_descriptor() 创建
//   - Page descriptor (L3):     含物理页地址 + 内存属性，用 new() 创建
//
// 创建映射的完整流程:
//   1. find_or_create_pte_vpn() 沿 L1→L2→L3 路径查找/创建中间级 table descriptor
//   2. 到达 L3 后，用 PageTableEntry::new(ppn, flags) 创建最终的 page descriptor
//   3. flags_to_aarch64() 将平台无关的 PTEFlags 翻译为 AArch64 硬件属性位
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct PageTableEntry(pub usize);

impl PageTableEntry {
    /// 创建 L3 page descriptor（最终页映射）
    ///
    /// 构建步骤:
    ///   1. 如果 V 标志有效，设置 bits[1:0]=0b11 (DESC_TYPE_PAGE)
    ///   2. 将物理页号左移 12 位，写入 bits[47:12]
    ///   3. 调用 flags_to_aarch64() 翻译所有内存属性 (AP/SH/AF/nG/AttrIdx/PXN/UXN/DBM)
    ///
    /// 来源: Linux pgtable-hwdef.h PTE_TYPE_PAGE
    /// 注意: 始终设置 AF (Access Flag)，避免 Access Flag Fault
    /// Linux 在 PAGE_KERNEL / PAGE_SHARED 等宏中始终包含 PTE_AF
    pub fn new(ppn: usize, flags: PTEFlags) -> Self {
        let mut desc: usize = 0;

        if flags.contains(PTEFlags::V) {
            // L3 page descriptor: bits[1:0] = 0b11
            desc |= DESC_TYPE_PAGE;
        }

        // 物理地址 bits[47:12]
        desc |= (ppn << 12) & PTE_ADDR_MASK;

        // 翻译平台无关标志为 AArch64 硬件属性
        desc |= Self::flags_to_aarch64(flags);

        PageTableEntry(desc)
    }

    /// 创建中间级 table descriptor (L1/L2 指向下一级页表)
    ///
    /// 构建步骤:
    ///   1. 将下一级页表的物理页号左移 12 位，得到物理地址
    ///   2. 设置 bits[1:0]=0b11 (DESC_TYPE_TABLE)
    ///   3. 完成! 不设置任何页级属性
    ///
    /// 来源: Linux head.S create_table_entry 宏
    ///   orr tmp2, tmp2, #PMD_TYPE_TABLE
    ///
    /// 关键: table descriptor 只需要 bits[1:0]=0b11 + 物理地址
    /// 不包含 AP/SH/nG/AttrIdx/PXN/UXN 等页级属性
    /// (之前的 bug: 用 new() 创建中间级，导致页属性污染 table descriptor)
    pub fn new_table_descriptor(ppn: usize) -> Self {
        let addr = (ppn << 12) & PTE_ADDR_MASK;
        PageTableEntry(DESC_TYPE_TABLE | addr)
    }

    /// 将平台无关 PTEFlags 翻译为 AArch64 硬件属性位
    ///
    /// 翻译顺序 (与 Linux pgprot_val 构建顺序一致):
    ///   1. AttrIndx[4:2] — 选择 MAIR 中的内存类型 (Normal 或 Device)
    ///   2. AP[2:1] bits[7:6] — 访问权限 (RW/RO, 内核/用户)
    ///   3. SH[9:8] — 共享属性 (Inner Shareable)
    ///   4. AF bit[10] — Access Flag (无条件设置)
    ///   5. nG bit[11] — non-Global (用户空间映射)
    ///   6. PXN/UXN bits[53:54] — Execute-Never (W^X 安全策略)
    ///   7. DBM bit[51] — Dirty Bit Management
    ///
    /// 来源: Linux pgtable-hwdef.h 各 PTE_xxx 定义
    fn flags_to_aarch64(flags: PTEFlags) -> usize {
        let mut attr = 0usize;

        // AttrIndx[3:2] — 选择 MAIR 中的内存类型
        // DEV → AttrIdx=1 (Device nGnRE, MAIR Attr1=0x04)
        // 否则 → AttrIdx=0 (Normal WB, MAIR Attr0=0xFF)
        if flags.contains(PTEFlags::DEV) {
            attr |= PTE_ATTRINDX_DEVICE;
        } else {
            attr |= PTE_ATTRINDX_NORMAL;
        }

        // AP[2:1] bits[7:6] — 访问权限
        // 来源: docs/aarch64_page_table.md §1.5
        //   AP[2]=0,AP[1]=0 → EL1:RW, EL0:无
        //   AP[2]=0,AP[1]=1 → EL1:RW, EL0:RW
        //   AP[2]=1,AP[1]=0 → EL1:RO, EL0:无
        //   AP[2]=1,AP[1]=1 → EL1:RO, EL0:RO
        if flags.contains(PTEFlags::U) {
            attr |= PTE_USER; // AP[1]=1
            if !flags.contains(PTEFlags::W) {
                attr |= PTE_RDONLY; // AP[2]=1 → 只读
            }
        } else {
            if !flags.contains(PTEFlags::W) {
                attr |= PTE_RDONLY; // AP[2]=1 → 内核只读
            }
        }

        // SH[9:8] — Inner Shareable
        // 来源: pgtable-hwdef.h PTE_SHARED = 3 << 8
        attr |= PTE_SHARED;

        // AF bit[10] — Access Flag
        // 始终设置，避免 Access Flag Fault
        // 来源: Linux 在所有有效映射中始终包含 PTE_AF
        attr |= PTE_AF;

        // nG bit[11] — non-Global
        // G 标志表示全局映射（内核空间），nG=0
        // 非 G 则 nG=1（用户空间，与 ASID 关联）
        if !flags.contains(PTEFlags::G) {
            attr |= PTE_NG;
        }

        // PXN bit[53] / UXN bit[54] — Execute-Never
        // 来源: pgtable-hwdef.h PTE_PXN, PTE_UXN
        if !flags.contains(PTEFlags::X) {
            // 不可执行: 同时设置 PXN 和 UXN
            attr |= PTE_PXN;
            attr |= PTE_UXN;
        } else if !flags.contains(PTEFlags::U) {
            // 内核可执行但用户不可执行: 只设 UXN
            attr |= PTE_UXN;
        }

        // DBM bit[51] — Dirty Bit Management
        // 来源: pgtable-hwdef.h PTE_DBM
        if flags.contains(PTEFlags::D) {
            attr |= PTE_DBM;
        }

        attr
    }

    /// 从 AArch64 硬件描述符反向解码为平台无关 PTEFlags
    ///
    /// 解码顺序 (与 flags_to_aarch64 相反):
    ///   1. bits[1:0] → V (有效位)
    ///   2. AP[2:1] bits[7:6] → R/W/U (读写权限 + 用户态)
    ///   3. PXN bit[53] → X (可执行，PXN=0 表示内核可执行)
    ///   4. AF bit[10] → A (已访问)
    ///   5. nG bit[11] → G (全局映射，nG=0 → G=1)
    ///   6. DBM bit[51] → D (脏页)
    ///   7. AttrIndx[4:2] → DEV (设备内存，AttrIdx=1)
    pub fn flags(&self) -> PTEFlags {
        let mut flags = PTEFlags::empty();

        // 检查有效位 bits[1:0] == 0b11
        if (self.0 & 0b11) == 0b11 {
            flags |= PTEFlags::V;
        }

        // AP[2:1] bits[7:6] → R/W/U
        let ap = (self.0 >> 6) & 0b11;
        match ap {
            0b00 => { flags |= PTEFlags::R | PTEFlags::W; }         // EL1:RW
            0b01 => { flags |= PTEFlags::R | PTEFlags::W | PTEFlags::U; } // EL1:RW, EL0:RW
            0b10 => { flags |= PTEFlags::R; }                       // EL1:RO
            0b11 => { flags |= PTEFlags::R | PTEFlags::U; }         // EL1:RO, EL0:RO
            _ => {}
        }

        // PXN bit[53] → X
        // PXN=0 表示内核可执行
        let pxn = (self.0 >> 53) & 1;
        if pxn == 0 {
            flags |= PTEFlags::X;
        }

        // AF bit[10] → A
        if (self.0 >> 10) & 1 == 1 {
            flags |= PTEFlags::A;
        }

        // nG bit[11] → G (取反)
        if (self.0 >> 11) & 1 == 0 {
            flags |= PTEFlags::G;
        }

        // DBM bit[51] → D
        if (self.0 >> 51) & 1 == 1 {
            flags |= PTEFlags::D;
        }

        // AttrIndx[3:2] → DEV
        let attrindx = (self.0 >> PTE_ATTRINDX_SHIFT) & 0b111;
        if attrindx == 1 {
            flags |= PTEFlags::DEV;
        }

        flags
    }

    pub fn set_flags(&mut self, flags: PTEFlags) {
        let ppn = self.ppn().0;
        *self = Self::new(ppn, flags);
    }

    /// 提取物理页号 (bits[47:12] >> 12)
    pub fn ppn(&self) -> PhysiNumber {
        PhysiNumber((self.0 & PTE_ADDR_MASK) >> 12)
    }

    /// 检查描述符是否有效 (bits[1:0] == 0b11)
    pub fn is_valid(&self) -> bool {
        (self.0 & 0b11) == 0b11
    }

    pub fn set_inValid(&mut self) {
        self.0 = 0;
    }

    pub fn set_isdirty(&mut self) {
        self.0 |= PTE_DBM;
    }

    pub fn set_isaccess(&mut self) {
        self.0 |= PTE_AF;
    }

    /// 输出描述符的完整位域解析（用于调试）
    pub fn dump_descriptor(&self, prefix: &str) {
        let raw = self.0;
        let valid = raw & 0b11;
        let attrindx = (raw >> 2) & 0b111;
        let ap = (raw >> 6) & 0b11;
        let sh = (raw >> 8) & 0b11;
        let af = (raw >> 10) & 1;
        let ng = (raw >> 11) & 1;
        let addr = (raw & PTE_ADDR_MASK) >> 12;
        let dbm = (raw >> 51) & 1;
        let pxn = (raw >> 53) & 1;
        let uxn = (raw >> 54) & 1;

        debug!(
            "{} raw={:#018x} type={:#04b} addr={:#x} AttrIdx={} AP={:#04b} SH={:#04b} AF={} nG={} DBM={} PXN={} UXN={}",
            prefix, raw, valid, addr, attrindx, ap, sh, af, ng, dbm, pxn, uxn
        );
    }
}

// =============================================================================
// 页表 (PageTable)
// =============================================================================
//
// 管理一棵 3 级页表树 (L1→L2→L3)。
//
// 结构:
//   root_ppn: L1 页表的物理页号，写入 TTBR0_EL1 后 MMU 从这里开始翻译
//   entries:  持有所有分配的页表页帧的 FramTracker，防止被回收
//
// 地址翻译过程 (MMU 硬件执行):
//   1. 从 TTBR0_EL1 读取 L1 页表基地址
//   2. 用 VA[38:30] (9 bits) 索引 L1 表，得到 L2 table descriptor
//   3. 从 L2 table descriptor 提取 L2 页表地址
//   4. 用 VA[29:21] (9 bits) 索引 L2 表，得到 L3 table descriptor
//   5. 从 L3 table descriptor 提取 L3 页表地址
//   6. 用 VA[20:12] (9 bits) 索引 L3 表，得到 page descriptor
//   7. 从 page descriptor 提取物理页地址，拼接 VA[11:0] 偏移，得到最终物理地址
#[derive(Clone)]
pub struct PageTable {
    pub root_ppn: PhysiNumber,
    entries: Vec<FramTracker>,
}

impl PageTable {
    pub fn new() -> Self {
        let root_frame = alloc_frame().expect("failed to alloc frame for page table");
        PageTable {
            root_ppn: PhysiNumber(root_frame.ppn.0),
            entries: vec![root_frame],
        }
    }

    /// 从当前 TTBR0_EL1 获取内核页表
    /// 修复: 之前错误地读取 TTBR1_EL1，但 active_memset() 将内核表写入 TTBR0_EL1
    /// 且 TCR_EL1.EPD1=1 已禁用 TTBR1 翻译
    pub fn get_kernel_table_layer() -> PageTable {
        let ttbr0: usize;
        unsafe {
            core::arch::asm!("mrs {0}, ttbr0_el1", out(reg) ttbr0);
        }
        PageTable {
            root_ppn: PhysiNumber((ttbr0 & PTE_ADDR_MASK) >> 12),
            entries: Vec::new(),
        }
    }

    pub fn get_mut_slice_from_satp(
        satp: usize,
        len: usize,
        startAddr: VirAddr,
    ) -> Vec<&'static mut [u8]> {
        let mut start_addr = startAddr;
        let end_addr = VirAddr(start_addr.0 + len);
        let mut table = PageTable::crate_table_from_satp(satp);

        let mut result_v = Vec::new();
        while start_addr < end_addr {
            let start_vpn = start_addr.floor_down();
            let source_slice = table
                .get_mut_byte(start_vpn.into())
                .expect("Get VPN to RealAddr failed");
            let page_end_addr: VirAddr = VirNumber(start_vpn.0 + 1).into();
            let real_end_addr = page_end_addr.min(end_addr);

            let start_offset = start_addr.offset();
            let end_offset = if real_end_addr.0 / PAGE_SIZE == start_vpn.0 {
                real_end_addr.offset()
            } else {
                PAGE_SIZE
            };

            result_v.push(&mut source_slice[start_offset..end_offset]);
            start_addr = real_end_addr;
        }

        result_v
    }

    pub fn read_bytes_from_userspace(&mut self, vidr: VirAddr, len: usize) -> Option<Vec<u8>> {
        let mut byt: Vec<u8> = Vec::new();
        for i in 0..len {
            match self.translate(VirAddr(vidr.0 + i)) {
                Some(phy) => {
                    byt.push(unsafe { *(phy.0 as *const u8) });
                }
                None => {
                    return None;
                }
            }
        }
        Some(byt)
    }

    pub fn crate_table_from_satp(satp: usize) -> Self {
        PageTable {
            root_ppn: PhysiNumber((satp & PTE_ADDR_MASK) >> 12),
            entries: Vec::new(),
        }
    }

    pub fn get_mut_byte(&mut self, vpn: VirNumber) -> Option<&'static mut [u8; PAGE_SIZE]> {
        let phydr = self
            .translate(vpn.into())
            .expect("Virtual Address translate to Physical Adress Failed!");
        unsafe {
            Some(
                core::slice::from_raw_parts_mut(phydr.0 as *mut u8, PAGE_SIZE)
                    .try_into()
                    .expect("GET_MUT_BYTE FAILED TO TRY TRANSLATE A  POINTER TO STATIC PAGESIZE"),
            )
        }
    }

    pub fn translate(&mut self, VDDR: VirAddr) -> Option<PhysiAddr> {
        match self.find_pte_vpn(VDDR.into()) {
            Some(pte) => {
                let ppn = pte.ppn();
                let addr = (ppn.0 * PAGE_SIZE) + VDDR.offset();
                Some(PhysiAddr(addr))
            }
            None => None,
        }
    }

    pub fn translate_byvpn(&mut self, vpn: VirNumber) -> Option<PhysiNumber> {
        compiler_fence(Ordering::SeqCst);
        match self.find_pte_vpn(vpn.into()) {
            Some(pte) => {
                let ppn = pte.ppn();
                compiler_fence(Ordering::SeqCst);
                Some(ppn)
            }
            None => None,
        }
    }

    pub fn get_pte_array(&self, phynum: usize) -> &'static mut [PageTableEntry; 512] {
        let phyaddr: PhysiAddr = PhysiNumber(phynum).into();
        unsafe {
            core::slice::from_raw_parts_mut(phyaddr.0 as *mut PageTableEntry, 512)
                .try_into()
                .expect("GET_PET_ARRAY FAILED ,WHEN TRANSLATE A POINTER TO 512 SIZE")
        }
    }

    pub fn find_pte_vpn(&mut self, VirNum: VirNumber) -> Option<&mut PageTableEntry> {
        let mut current_ppn = self.root_ppn.0;
        let idx = VirNum.index();
        let mut pte_array = self.get_pte_array(current_ppn);

        for (id, index) in idx.iter().enumerate() {
            compiler_fence(Ordering::SeqCst);
            let entry = &mut pte_array[*index];

            if id == 2 {
                return Some(entry);
            }
            if !entry.is_valid() {
                return None;
            }
            current_ppn = entry.ppn().0;
            pte_array = self.get_pte_array(current_ppn);
        }

        None
    }

    /// 建立虚拟页 → 物理页的映射
    ///
    /// 步骤:
    ///   1. find_or_create_pte_vpn() 沿 L1→L2→L3 路径查找/创建中间级
    ///   2. 如果 L3 PTE 已有效，合并新旧标志 (OR 操作)
    ///   3. 如果 L3 PTE 无效，用 new() 创建新的 page descriptor
    pub fn map(&mut self, vpn: VirNumber, ppn: PhysiNumber, flags: PTEFlags) {
        let pte = self.find_or_create_pte_vpn(vpn).expect("Failed When Map");

        if pte.is_valid() {
            pte.set_flags(flags | pte.flags());
            return;
        }

        *pte = PageTableEntry::new(ppn.0, flags | PTEFlags::V);
    }

    pub fn is_maped(&mut self, vpn: VirNumber) -> bool {
        match self.find_pte_vpn(vpn) {
            Some(pte) => pte.is_valid(),
            None => false,
        }
    }

    pub fn unmap(&mut self, vpn: VirNumber) {
        let pte = self.find_pte_vpn(vpn);
        match pte {
            Some(pte) => {
                if pte.is_valid() {
                    pte.set_inValid();
                } else {
                    error!("This PTE is Invalid");
                }
            }
            None => {
                error!("Unmap failed!No PTE find to unmap");
            }
        }
    }

    /// 查找或创建到 L3 PTE 的路径
    ///
    /// 遍历流程:
    ///   id=0: 用 L1 索引查找 L1 表 → 如果无效，分配新页帧作为 L2 表，
    ///         用 new_table_descriptor() 创建 L1→L2 的 table descriptor
    ///   id=1: 用 L2 索引查找 L2 表 → 如果无效，分配新页帧作为 L3 表，
    ///         用 new_table_descriptor() 创建 L2→L3 的 table descriptor
    ///   id=2: 用 L3 索引找到最终 PTE 位置 → 返回可变引用，由调用者填写 page descriptor
    ///
    /// 修复: 中间级使用 new_table_descriptor() 而非 new()
    /// 来源: Linux head.S create_table_entry — table descriptor 只需 TYPE_TABLE + 地址
    fn find_or_create_pte_vpn(&mut self, VirNum: VirNumber) -> Option<&mut PageTableEntry> {
        let mut current_ppn = self.root_ppn.0;
        let idx = VirNum.index();
        let mut pte_array = self.get_pte_array(current_ppn);
        for (id, index) in idx.iter().enumerate() {
            let entry = &mut pte_array[*index];

            if id == 2 {
                return Some(entry);
            }
            if !entry.is_valid() {
                let frame = alloc_frame().expect("Frame alloc failed on pte alloc");
                let ppn = frame.ppn.0;
                // 关键修复: 使用 table descriptor，不注入页属性
                *entry = PageTableEntry::new_table_descriptor(ppn);
                self.entries.push(frame);
            }
            current_ppn = entry.ppn().0;
            pte_array = self.get_pte_array(current_ppn);
        }
        None
    }

    /// 返回页表根物理地址（用于写入 TTBR0_EL1）
    ///
    /// TTBR0_EL1 存储 L1 页表的物理基地址。
    /// MMU 开启后，所有虚拟地址翻译都从这个地址开始。
    /// 注意: AArch64 的 TTBR 直接存储物理地址，不像 RISC-V 的 satp 需要模式位。
    pub fn satp_token(&self) -> usize {
        self.root_ppn.0 << 12
    }
}
