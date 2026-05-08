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

// ─── BAR 尺寸探测常量 ───────────────────────────────────────────────

/// 写入全 1 进行 BAR sizing 探测
const BAR_SIZING_PROBE: u32 = 0xFFFF_FFFF;

/// BAR（基地址寄存器）类型
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarKind {
    /// I/O 端口空间 BAR。
    Io,
    /// 32 位 memory BAR。
    Mem32,
    /// 64 位 memory BAR。
    Mem64,
}

impl BarKind {
    /// 从 BAR 原始值中解码 BAR 类型。
    ///
    /// 参考 Linux 5.4.29:
    /// - `drivers/pci/probe.c:126-150 decode_bar()`
    /// - `include/uapi/linux/pci_regs.h:93-108`
    pub fn from_raw(raw: u32) -> Option<Self> {
        if raw & BAR_IO_BIT != 0 {
            return Some(Self::Io);
        }

        match (raw >> BAR_MEM_TYPE_SHIFT) & BAR_MEM_TYPE_MASK {
            BAR_MEM_TYPE_32BIT => Some(Self::Mem32),
            BAR_MEM_TYPE_64BIT => Some(Self::Mem64),
            _ => None,
        }
    }

    /// 返回低 32 位 BAR 中真正表示地址的比特掩码。
    fn low_address_mask(self) -> u32 {
        match self {
            Self::Io => BAR_IO_ADDR_MASK,
            Self::Mem32 | Self::Mem64 => BAR_MEM_ADDR_MASK,
        }
    }

    /// 返回 Linux `pci_size()` 计算时使用的完整地址掩码。
    fn full_address_mask(self) -> u64 {
        match self {
            Self::Io => BAR_IO_ADDR_MASK as u64,
            Self::Mem32 => BAR_MEM_ADDR_MASK as u64,
            Self::Mem64 => (BAR_MEM_ADDR_MASK as u64) | ((u32::MAX as u64) << 32),
        }
    }

    /// 当前 BAR 是否占用两个 32 位槽位。
    fn is_64(self) -> bool {
        matches!(self, Self::Mem64)
    }

    /// 该 BAR 需要打开的 PCI_COMMAND decode 位。
    pub fn command_decode_bit(self) -> u16 {
        match self {
            Self::Io => PCI_COMMAND_IO,
            Self::Mem32 | Self::Mem64 => PCI_COMMAND_MEMORY,
        }
    }
}

/// PCI BAR 描述结构
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PciBar {
    /// BAR 当前配置空间里的原始值。
    raw: u32,
    /// BAR 类型。
    pub kind: BarKind,
    /// 当前 BAR 的基地址。
    pub base: u64,
    /// 当前 BAR 是否标记为 prefetchable。
    pub prefetchable: bool,
}

/// 解码 BAR 原始值，返回 PciBar 结构
/// `next_raw` 仅在 64 位 BAR 时需要，提供高 32 位
pub fn decode_bar(raw: u32, next_raw: Option<u32>) -> Option<PciBar> {
    if raw == 0 {
        return None;
    }

    let bar_kind = BarKind::from_raw(raw)?;
    // bit3 表示是否支持预取
    let prefetchable = (raw & BAR_PREFETCH_BIT) != 0;

    match bar_kind {
        BarKind::Io => Some(PciBar {
            raw,
            kind: BarKind::Io,
            base: (raw & BAR_IO_ADDR_MASK) as u64,
            prefetchable: false,
        }),
        BarKind::Mem32 => Some(PciBar {
            raw,
            kind: BarKind::Mem32,
            base: (raw & BAR_MEM_ADDR_MASK) as u64,
            prefetchable,
        }),
        BarKind::Mem64 => {
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
    }
}

/// 对齐到 `alignment` 的上边界。
fn align_up(value: u64, alignment: u64) -> u64 {
    (value + alignment - 1) & !(alignment - 1)
}

/// 从 PCI MMIO 地址池中分配一块大小为 size 的区域
/// 返回分配的基地址
///
/// TODO(dirinkbottle):
/// 1. 当前只是最简单的线性 bump allocator，不支持释放和重平衡；
/// 2. 当前统一从 32 位 MMIO 窗口里分配，后面如果要支持 4 GiB 以上 BAR，
///    需要单独引入 64 位地址分配策略。
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

    let bar_kind = BarKind::from_raw(raw)?;
    if bar_kind == BarKind::Io {
        return None;
    }

    let (size, original_command) = bar_size(bus, dev, func, PCI_BAR0, bar_kind);
    if size == 0 {
        pci_enable_device(bus, dev, func, original_command);
        return None;
    }

    // 分配 MMIO 地址空间
    let base = alloc_pci_mmio(size);

    // 写入低 32 位基地址（保留原始低 4 位控制字段）
    let low = (base as u32) & BAR_MEM_ADDR_MASK;
    unsafe { cfg_write32(bus, dev, func, PCI_BAR0, low | (raw & BAR_ATTR_MASK)) };

    // 64 位 BAR 需写入高 32 位
    if bar_kind.is_64() {
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

    pci_enable_device(
        bus,
        dev,
        func,
        original_command | bar_kind.command_decode_bit(),
    );

    Some((base, size))
}

/// 探测 BAR 大小
/// 原理：向 BAR 写入全 1，读回后低位的控制比特会被硬件清零，
/// 取反加 1 即可得到 BAR 的地址空间大小
///
/// 返回值：
/// 1. `u64` 是探测到的 BAR 大小；
/// 2. `u16` 是 sizing 前读取到的原始 `PCI_COMMAND`，供外层在 BAR 回写完成后恢复 decode。
///
/// 参考 Linux 5.4.29：
/// - `drivers/pci/probe.c:109-128 pci_size()`
/// - `drivers/pci/probe.c:175-258 __pci_read_base()`
/// - `drivers/pci/probe.c:320-335 pci_read_bases()`
/// - `include/uapi/linux/pci_regs.h:39-42, 61-80`
///
/// 与 Linux 的差异：
/// 1. Linux 在 `__pci_read_base()` 结束前会恢复 `PCI_COMMAND`；
/// 2. 本项目把“探测 size”和“立刻分配并回写 BAR”合并在同一阶段；
/// 3. 因此这里返回 `old_cmd`，并故意让 decode 继续保持关闭，交给调用者在 BAR 回写后恢复。
pub fn bar_size(bus: u8, dev: u8, func: u8, bar_off: u16, bar_kind: BarKind) -> (u64, u16) {
    // 按 Linux probe.c:pci_size() 109-128 计算 BAR 大小。
    fn pci_bar_size(base: u64, max_base: u64, address_mask: u64) -> u64 {
        let mut size = address_mask & max_base;
        if size == 0 {
            return 0;
        }

        // 提取最低有效位，得到 decode granularity。
        size &= !(size - 1);

        // Linux 这里会额外过滤“base == max_base 但实际上不是合法 BAR”的情况。
        if base == max_base && ((base | (size - 1)) & address_mask) != address_mask {
            return 0;
        }

        size
    }

    // 参考:
    //   1. drivers/pci/probe.c:__pci_read_base() 175-258
    //   2. drivers/pci/probe.c:pci_read_bases() 320-335
    //   3. include/uapi/linux/pci_regs.h 39-42, 93-108
    //
    // Linux 的 BAR sizing 关键点不是“直接写 0xffffffff 再读”，而是下面这个完整流程:
    //
    //   Step 0. 先读 PCI_COMMAND，记录原始 decode 状态。
    //           见 __pci_read_base() 180, 187-191。
    //           如果 IO/MEM decode 开着，先暂时清掉 PCI_COMMAND_IO / PCI_COMMAND_MEMORY。
    //           原因: sizing 期间 BAR 会短暂变成全 1，如果还在 decode，设备可能错误响应总线周期。
    //
    //   Step 1. 读取 BAR 当前值 l/raw。
    //           见 __pci_read_base() 196。
    //           这个值只表示当前编程地址和属性位，不是“BAR 是否实现”的最终判据。
    //
    //   Step 2. 向 BAR 写入全 1，再读回 sz/mask，然后恢复原值。
    //           见 __pci_read_base() 197-199。
    //
    //   Step 3. 用 sz/mask 判断 BAR 是否实现。
    //           见 __pci_read_base() 201-208, 251-258。
    //           Linux 的核心判据是“probe 回来的 size mask 是否为 0”，
    //           不是 old/raw 是否为 0。也就是说:
    //             old == 0   -> 可能只是还没分配地址
    //             mask == 0  -> 才能认为未实现
    //
    //   Step 4. 用 mask 去算 size，而不是用 old/raw 去算。
    //           见 pci_size() 109-128 与 __pci_read_base() 254。
    //
    //   Step 5. 如果一开始关掉了 decode，这里要把 PCI_COMMAND 恢复。
    //           见 __pci_read_base() 248-249。
    //
    // 你后面在 probe_device_bars() 里要改的是:
    //   “先 size，再决定跳过”，而不是“先看 raw，再决定是否 size”。
    //
    // 进一步要补的点:
    //   1. 32-bit / 64-bit BAR 最好共用一套 sizing 入口，避免重复逻辑。
    //   2. 64-bit BAR 需要同时 probe 高 32 位，并在外层循环多跳过一个槽位。
    //   3. 如果将来要更接近 Linux，可以把“探测资源(resource discovery)”和“分配地址(resource assignment)”
    //      分成两个阶段；Linux 在 probe.c 里先读 resource，在 setup-res.c 里再分配/回写。

    // 先关掉 IO/MEM decode，避免 sizing 期间 BAR=0xffffffff 时错误响应总线周期。
    let old_cmd = pci_disable_device(bus, dev, func);

    // 保存 BAR 原始值，后面必须完整恢复。
    let mut original_low = unsafe { cfg_read32(bus, dev, func, bar_off) };
    let mut original_high = 0;
    if bar_kind.is_64() {
        original_high = unsafe { cfg_read32(bus, dev, func, bar_off + PCI_BAR_SPACING) };
    }

    // 写全 1 探测可实现的地址位。
    unsafe { cfg_write32(bus, dev, func, bar_off, BAR_SIZING_PROBE) };
    if bar_kind.is_64() {
        unsafe { cfg_write32(bus, dev, func, bar_off + PCI_BAR_SPACING, BAR_SIZING_PROBE) };
    }

    let mut size_low = unsafe { cfg_read32(bus, dev, func, bar_off) };
    let mut size_high = 0;
    if bar_kind.is_64() {
        size_high = unsafe { cfg_read32(bus, dev, func, bar_off + PCI_BAR_SPACING) };
    }

    // 无论 probe 结果如何，都先恢复 BAR 原始内容。
    unsafe { cfg_write32(bus, dev, func, bar_off, original_low) };
    if bar_kind.is_64() {
        unsafe { cfg_write32(bus, dev, func, bar_off + PCI_BAR_SPACING, original_high) };
    }

    // Linux __pci_read_base() 201-208: probe 回来全 1 视为无效，先归零再参与 size 计算。
    if size_low == BAR_SIZING_PROBE {
        size_low = 0;
    }
    if original_low == BAR_SIZING_PROBE {
        original_low = 0;
    }

    let mut base_address = (original_low & bar_kind.low_address_mask()) as u64;
    let mut size_mask = (size_low & bar_kind.low_address_mask()) as u64;
    if bar_kind.is_64() {
        base_address |= (original_high as u64) << 32;
        size_mask |= (size_high as u64) << 32;
    }

    if size_mask == 0 {
        return (0, old_cmd);
    }

    (
        pci_bar_size(base_address, size_mask, bar_kind.full_address_mask()),
        old_cmd,
    )
}
