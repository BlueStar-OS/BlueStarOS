use crate::driver::pcie::*;
use crate::pcie_log;

// ─── PCI MMIO 分配器 ────────────────────────────────────────────────
// 为 PCI BAR 分配 MMIO 地址空间，从 PCI_MMIO_ALLOC_BASE 向上增长
// ────────────────────────────────────────────────────────────────────

/// PCI BAR MMIO 地址空间起始地址（1GB 窗口低端）
const PCI_MMIO_ALLOC_BASE: u64 = 0x4000_0000;
/// PCI BAR MMIO 地址空间结束地址
const PCI_MMIO_ALLOC_END: u64 = 0x8000_0000;

/// MMIO 分配器当前指针，从 PCI_MMIO_ALLOC_BASE 开始向上增长
static mut PCI_MMIO_ALLOC: u64 = PCI_MMIO_ALLOC_BASE;

// ─── BAR 属性位定义（PCI 规范 7.5.1.2） ─────────────────────────────

/// I/O 空间 BAR 标志位：bit0 = 1 表示 I/O 空间，0 表示 Memory 空间
pub const BAR_IO_BIT: u32 = 0x1;
/// Memory 空间类型编码位移位（bits[2:1]）
pub const BAR_MEM_TYPE_SHIFT: u32 = 1;
/// Memory 空间类型编码掩码（2 位）
pub const BAR_MEM_TYPE_MASK: u32 = 0x3;
/// 预取使能位（bit3）
pub const BAR_PREFETCH_BIT: u32 = 0x8;

// BAR Memory 类型编码值
/// 32 位 Memory BAR（mem_type = 0b00）
pub const BAR_MEM_TYPE_32BIT: u32 = 0b00;
/// 64 位 Memory BAR（mem_type = 0b10）
pub const BAR_MEM_TYPE_64BIT: u32 = 0b10;

/// I/O BAR 基地址掩码（低 2 位为控制位，不可寻址）
pub const BAR_IO_ADDR_MASK: u32 = 0xFFFF_FFFC;
/// Memory BAR 基地址掩码（低 4 位为控制/属性位，不可寻址）
pub const BAR_MEM_ADDR_MASK: u32 = 0xFFFF_FFF0;
/// BAR 低 4 位属性位掩码：bit0=I/O标志, bits[2:1]=mem类型, bit3=预取
pub const BAR_ATTR_MASK: u32 = 0xF;

// 用于判断 64 位 BAR 的组合条件：(mem_type == 0b10) << 1
const BAR_64BIT_CHECK_MASK: u32 = (BAR_MEM_TYPE_MASK << BAR_MEM_TYPE_SHIFT);
const BAR_64BIT_CHECK_VALUE: u32 = (BAR_MEM_TYPE_64BIT << BAR_MEM_TYPE_SHIFT);

// ─── BAR 尺寸探测常量 ───────────────────────────────────────────────

/// 写入全 1 进行 BAR sizing 探测
const BAR_SIZING_PROBE: u32 = 0xFFFF_FFFF;
/// I/O BAR 尺寸掩码（低 2 位控制位清零）
const BAR_IO_SIZE_MASK: u32 = 0xFFFF_FFFC;
/// Memory BAR 尺寸掩码（低 4 位控制/属性位清零）
const BAR_MEM_SIZE_MASK: u32 = 0xFFFF_FFF0;

/// BAR（基地址寄存器）类型
#[derive(Clone, Copy)]
pub enum BarKind {
    Io,    // I/O 端口空间
    Mem32, // 32 位内存空间
    Mem64, // 64 位内存空间
}

/// PCI BAR 描述结构
pub struct PciBar {
    raw: u32,               // BAR 原始值
    pub kind: BarKind,      // BAR 类型
    pub base: u64,          // 基地址
    pub prefetchable: bool, // 是否支持预取
}

/// 解码 BAR 原始值，返回 PciBar 结构
/// `next_raw` 仅在 64 位 BAR 时需要，提供高 32 位
pub fn decode_bar(raw: u32, next_raw: Option<u32>) -> Option<PciBar> {
    if raw == 0 {
        return None;
    }

    // bit0 = 1 表示 I/O 空间 BAR
    if raw & BAR_IO_BIT != 0 {
        return Some(PciBar {
            raw,
            kind: BarKind::Io,
            base: (raw & BAR_IO_ADDR_MASK) as u64,
            prefetchable: false,
        });
    }

    // 内存空间 BAR：bits[2:1] 编码内存类型
    let mem_type = (raw >> BAR_MEM_TYPE_SHIFT) & BAR_MEM_TYPE_MASK;
    // bit3 表示是否支持预取
    let prefetchable = (raw & BAR_PREFETCH_BIT) != 0;

    match mem_type {
        BAR_MEM_TYPE_32BIT => Some(PciBar {
            raw,
            kind: BarKind::Mem32,
            base: (raw & BAR_MEM_ADDR_MASK) as u64,
            prefetchable,
        }),
        BAR_MEM_TYPE_64BIT => {
            // 64 位 BAR：低 32 位在 raw，高 32 位在 next_raw
            let hi = next_raw.unwrap_or(0) as u64;
            let lo = (raw & BAR_MEM_ADDR_MASK) as u64;
            Some(PciBar {
                raw,
                kind: BarKind::Mem64,
                base: (hi << 32) | lo,
                prefetchable,
            })
        }
        _ => None,
    }
}

/// 按 align 对齐向上取整
fn align_up(x: u64, align: u64) -> u64 {
    (x + align - 1) & !(align - 1)
}

/// 从 PCI MMIO 地址池中分配一块大小为 size 的区域
/// 返回分配的基地址
pub fn alloc_pci_mmio(size: u64) -> u64 {
    unsafe {
        let base = align_up(PCI_MMIO_ALLOC, size);
        PCI_MMIO_ALLOC = base + size;
        assert!(PCI_MMIO_ALLOC <= PCI_MMIO_ALLOC_END);
        pcie_log!("bar alloc memory {:#x} bytes on addrress:{:#x}", size, base);
        base
    }
}

/// 为 BAR0 分配 MMIO 地址空间并写入设备
/// 返回 (基地址, 大小)
pub fn assign_bar0(bus: u8, dev: u8, func: u8) -> Option<(u64, u64)> {
    let raw = unsafe { cfg_read32(bus, dev, func, PCI_BAR0) };
    // 跳过 I/O 空间 BAR
    if raw & BAR_IO_BIT != 0 {
        return None;
    }

    // 检查是否为 64 位 BAR：mem_type == 0b10
    let is_64 = (raw & BAR_64BIT_CHECK_MASK) == BAR_64BIT_CHECK_VALUE;
    let size = bar_size32(bus, dev, func, PCI_BAR0) as u64;
    if size == 0 {
        return None;
    }

    // 分配 MMIO 地址空间
    let base = alloc_pci_mmio(size);

    // 写入低 32 位基地址（保留原始低 4 位控制字段）
    let low = (base as u32) & BAR_MEM_ADDR_MASK;
    unsafe { cfg_write32(bus, dev, func, PCI_BAR0, low | (raw & BAR_ATTR_MASK)) };

    // 64 位 BAR 需写入高 32 位
    if is_64 {
        unsafe {
            cfg_write32(
                bus,
                dev,
                func,
                PCI_BAR0 + PCI_BAR_SPACING,
                (base >> 32) as u32,
            )
        };
    }

    Some((base, size))
}

/// 探测 BAR 大小
/// 原理：向 BAR 写入全 1，读回后低位的控制比特会被硬件清零，
/// 取反加 1 即可得到 BAR 的地址空间大小
pub fn bar_size32(bus: u8, dev: u8, func: u8, bar_off: u16) -> u32 {
    // TODO: 探测大小前应先禁用设备命令寄存器

    // 保存 BAR 原始值
    let old = unsafe { cfg_read32(bus, dev, func, bar_off) };

    // 向 BAR 写入全 1
    unsafe { cfg_write32(bus, dev, func, bar_off, BAR_SIZING_PROBE) };
    let mask = unsafe { cfg_read32(bus, dev, func, bar_off) };

    // 恢复 BAR 原始值
    unsafe { cfg_write32(bus, dev, func, bar_off, old) };

    // 全 0 或全 1 表示 BAR 未实现
    if mask == 0 || mask == BAR_SIZING_PROBE {
        return 0;
    }
    // I/O BAR：低 2 位为控制位，不可寻址
    if mask & BAR_IO_BIT != 0 {
        let m = mask & BAR_IO_SIZE_MASK;
        (!m).wrapping_add(1)
    } else {
        // Memory BAR：低 4 位为控制位，不可寻址
        let m = mask & BAR_MEM_SIZE_MASK;
        (!m).wrapping_add(1)
    }
}
