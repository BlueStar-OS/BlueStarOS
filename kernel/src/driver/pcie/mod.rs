use crate::arch::memory::{PhysiAddr, VirAddr};
use crate::driver::pcie::bar::*;
use crate::driver::pcie::pcie_helper::*;
use crate::dtb::DeviceNode;
use crate::register_kernel_mmio;
use crate::MapAreaFlags;
use crate::VirNumRange;
use crate::dtb_probe;
use core::fmt;
use log::error;
mod bar;
mod pcie_helper;

// ─── PCI 配置空间标准寄存器偏移 ─────────────────────────────────────

/// 厂商 ID（16 位）
pub const PCI_VENDOR_ID: u16 = 0x00;
/// 设备 ID（16 位）
pub const PCI_DEVICE_ID: u16 = 0x02;
/// 命令寄存器（16 位）
pub const PCI_COMMAND: u16 = 0x04;
/// 类别码 [31:16] / 版本号 [15:0]（32 位）
pub const PCI_CLASS_REVISION: u16 = 0x08;
/// 头类型寄存器（8 位，bit7=多功能标志，bits[6:0]=布局代码）
pub const PCI_HEADER_TYPE: u16 = 0x0E;
/// BAR0 基地址寄存器偏移
pub const PCI_BAR0: u16 = 0x10;
/// 每个 BAR 寄存器占用的字节数
pub const PCI_BAR_SPACING: u16 = 4;

// ─── PCI 命令寄存器位定义 ────────────────────────────────────────────

/// 使能 I/O 空间访问
pub const PCI_COMMAND_IO: u16 = 0x1;
/// 使能内存空间访问
pub const PCI_COMMAND_MEMORY: u16 = 0x2;
/// 使能总线主控（DMA）
pub const PCI_COMMAND_MASTER: u16 = 0x4;

// ─── PCI 特殊值 ──────────────────────────────────────────────────────

/// 无效厂商 ID：表示该 PCI 槽位没有设备
const PCI_INVALID_VENDOR: u16 = 0xFFFF;
/// 类别码在 CLASS_REVISION 寄存器中的位移位
const PCI_CLASS_CODE_SHIFT: u32 = 8;

// ─── PCI 头类型 ──────────────────────────────────────────────────────

/// 头类型寄存器中用于屏蔽多功能标志位（bit7）的掩码
const PCI_HEADER_TYPE_MASK: u8 = 0x7F;
/// 多功能标志位（bit7 = 1 表示多功能设备）
const PCI_MULTIFUNCTION_BIT: u8 = 0x80;

/// 头类型布局代码：标准 PCI 端点/设备（Type 0）
const PCI_HEADER_TYPE_ENDPOINT: u8 = 0x00;
/// 头类型布局代码：PCI-to-PCI 桥（Type 1）
const PCI_HEADER_TYPE_BRIDGE: u8 = 0x01;

/// Type 0（端点）最多 6 个 BAR
const BAR_COUNT_ENDPOINT: u16 = 6;
/// Type 1（桥）最多 2 个 BAR
const BAR_COUNT_BRIDGE: u16 = 2;

/// PCI Header Type 寄存器（偏移 0x0E）的封装。
///
/// 寄存器布局：
///   bit7    — 多功能标志，1 表示设备支持多个功能
///   bits[6:0] — 配置空间布局代码（0x00=端点, 0x01=桥, 0x02=CardBus）
#[derive(Clone, Copy)]
struct HeaderType(u8);

impl HeaderType {
    /// 从原始寄存器值构造
    fn from_raw(raw: u8) -> Self {
        Self(raw)
    }

    /// 提取布局代码（bits 6:0），屏蔽多功能标志位
    fn layout(self) -> u8 {
        self.0 & PCI_HEADER_TYPE_MASK
    }

    /// 是否是多功能设备（bit7 置位）
    fn is_multifunction(self) -> bool {
        self.0 & PCI_MULTIFUNCTION_BIT != 0
    }

    /// 该头类型支持的 BAR 数量
    fn bar_count(self) -> u16 {
        match self.layout() {
            PCI_HEADER_TYPE_ENDPOINT => BAR_COUNT_ENDPOINT,
            PCI_HEADER_TYPE_BRIDGE => BAR_COUNT_BRIDGE,
            _ => 0, // CardBus 或保留类型，不扫描 BAR
        }
    }
}

impl fmt::LowerHex for HeaderType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::LowerHex::fmt(&self.0, f)
    }
}

// ─── PCIe MMIO 窗口 ──────────────────────────────────────────────────

/// PCIe BAR 映射窗口基地址（1GB 窗口低端）
const PCIE_WINDOW_MMIO_BASE: usize = 0x4000_0000;
/// PCIe BAR 映射窗口结束地址
const PCIE_WINDOW_MMIO_END: usize = 0x7FFF_FFFF;

// ─── 已知设备 ID ─────────────────────────────────────────────────────

/// QEMU edu 教学设备厂商 ID
const PCI_EDU_VENDOR: u16 = 0x1234;
/// QEMU edu 教学设备设备 ID
const PCI_EDU_DEVICE: u16 = 0x11E8;

// ─── 全局状态 ────────────────────────────────────────────────────────

/// 全局 ECAM 基地址，由设备树探测后填入
static mut PCIE_ECAM_ADDR: usize = 0;

// ─── 日志宏 ──────────────────────────────────────────────────────────

/// PCIe 日志宏，统一输出 `[PCIE]:` 前缀
#[macro_export]
macro_rules! pcie_log {
    ($fmt: literal $(, $($arg: tt)+)?) => {
        $crate::console::kprint(format_args!(concat!("[PCIE]: ",$fmt, "\n") $(, $($arg)+)?))
    }
}

// ─── 设备命令 ────────────────────────────────────────────────────────

/// 使能 PCIe 设备：开启内存空间访问和总线主控
pub fn pci_enable_device(bus: u8, dev: u8, func: u8) {
    let mut cmd = unsafe { cfg_read16(bus, dev, func, PCI_COMMAND) };
    cmd |= PCI_COMMAND_MEMORY | PCI_COMMAND_MASTER;
    unsafe { cfg_write16(bus, dev, func, PCI_COMMAND, cmd) };

    // 回读验证写入是否生效
    let new_cmd = unsafe { cfg_read16(bus, dev, func, PCI_COMMAND) };
    if new_cmd != cmd {
        error!(
            "[PCIE]: command set error, expect command {:#x} real_command:{:#x} ",
            cmd, new_cmd
        );
        return;
    }
    pcie_log!("PCI command: {:#x} -> {:#x}", cmd, new_cmd);
}

/// 禁用 PCIe 设备：关闭内存空间访问和总线主控
pub fn pci_disable_device(bus: u8, dev: u8, func: u8) {
    let mut cmd = unsafe { cfg_read16(bus, dev, func, PCI_COMMAND) };
    cmd &= !(PCI_COMMAND_MEMORY | PCI_COMMAND_MASTER);
    unsafe { cfg_write16(bus, dev, func, PCI_COMMAND, cmd) };

    // 回读验证写入是否生效
    let new_cmd = unsafe { cfg_read16(bus, dev, func, PCI_COMMAND) };
    if new_cmd != cmd {
        error!(
            "[PCIE]: command disable error, expect command {:#x} real_command:{:#x} ",
            cmd, new_cmd
        );
        return;
    }
    pcie_log!("PCI command: {:#x} -> {:#x}", cmd, new_cmd);
}

// ─── MMIO 空间注册 ──────────────────────────────────────────────────

/// 注册 PCIe MMIO 空间到内核 MMIO 子系统
/// 从设备树节点获取 ECAM 基地址并注册，同时注册 PCIe 窗口 MMIO 范围
fn pcie_register_pcie_mmio_space(node: &DeviceNode) {
    pcie_log!("Start register MMIO memory");

    // 从设备树获取 reg 属性（ECAM 基地址和大小）
    if let Ok(reg) = node.get_property("reg").ok_or("Missing reg property") {
        let regs = reg.as_reg(2, 2);
        if regs.is_empty() {
            error!("[PCIE] host regs is empty");
            return;
        }

        // 注册每个 ECAM MMIO 区域
        for reg in regs {
            pcie_log!("register one mmio region");
            let base_addr = reg.address as usize;
            let mmio_range = VirNumRange::new(
                VirAddr(base_addr),
                VirAddr(base_addr + (reg.size as usize) - 1),
            );
            // 设备内存：有效、可读、可写、分配、聚集
            let flags = MapAreaFlags::V
                | MapAreaFlags::R
                | MapAreaFlags::W
                | MapAreaFlags::A
                | MapAreaFlags::G
                | MapAreaFlags::DEV;
            register_kernel_mmio(mmio_range, flags);

            // 更新全局 ECAM 基地址
            unsafe {
                PCIE_ECAM_ADDR = reg.address as usize;
            }
        }
    } else {
        error!("[PCIE] node get property fail!");
    }

    // 注册 PCIe BAR 窗口 MMIO 范围（设备 BAR 空间将映射到此区域内）
    pcie_log!("register pcie window mmio range");
    let pcie_window_mmio_range = VirNumRange::new(
        VirAddr(PCIE_WINDOW_MMIO_BASE),
        VirAddr(PCIE_WINDOW_MMIO_END),
    );

    let flags = MapAreaFlags::V
        | MapAreaFlags::R
        | MapAreaFlags::W
        | MapAreaFlags::A
        | MapAreaFlags::G
        | MapAreaFlags::DEV;
    register_kernel_mmio(pcie_window_mmio_range, flags);
}

// ─── 总线扫描 ────────────────────────────────────────────────────────

/// 扫描总线 0 上的所有设备
/// 遍历每个设备的每个功能，读取并打印设备信息，探测并编程 BAR
fn scan_bus_0() {
    for dev in 0..DEVICE_PER_BUS {
        for func in 0..FUNCTION_PER_DEVICE {
            let vendor = unsafe { cfg_read16(0, dev, func, PCI_VENDOR_ID) };
            // 厂商 ID 为 0xffff 表示设备不存在
            if vendor == PCI_INVALID_VENDOR {
                if func == 0 {
                    break; // 功能 0 不存在则整个设备不存在
                }
                continue;
            }

            let device = unsafe { cfg_read16(0, dev, func, PCI_DEVICE_ID) };
            let class_rev = unsafe { cfg_read32(0, dev, func, PCI_CLASS_REVISION) };
            let class_code = class_rev >> PCI_CLASS_CODE_SHIFT;
            let header =
                HeaderType::from_raw(unsafe { cfg_read16(0, dev, func, PCI_HEADER_TYPE) as u8 });

            // TODO: 探测 edu 教学设备
            if vendor == PCI_EDU_VENDOR && device == PCI_EDU_DEVICE {
                pcie_log!("Find edu device!");
            }

            pcie_log!(
                "PCI 00:{:02x}.{} vendor={:x} device={:04x} class={:06x} header={:02x}",
                dev,
                func,
                vendor,
                device,
                class_code,
                header
            );

            // 根据头类型决定要扫描的 BAR 数量。
            // Type 0（端点）最多 6 个 BAR，Type 1（桥）最多 2 个 BAR。
            let max_bars = header.bar_count();

            // 扫描 BAR（基地址寄存器）。
            // BAR 在 PCI 配置空间从 BAR0(0x10) 开始，每个占 PCI_BAR_SPACING 字节。
            // 注意：64 位 Memory BAR 会占用连续两个 32 位槽位（低 + 高），
            // 第二个槽位应跳过，不能作为独立 BAR 再次解析。
            let mut bar: u16 = 0;
            while bar < max_bars {
                let off = PCI_BAR0 + bar * PCI_BAR_SPACING;

                // 读出 BAR 原始值：bit0=1 → I/O 空间，bit0=0 → Memory 空间；
                // Memory 空间由 bits[2:1] 区分 32 位(00) / 64 位(10)。
                let raw = unsafe { cfg_read32(0, dev, func, off) };

                // 全 0 表示该 BAR 未被设备实现，直接跳过。
                if raw == 0 {
                    bar += 1;
                    continue;
                }

                // --- I/O 空间 BAR -------------------------------------------------
                // bit0 = 1：I/O 端口空间。I/O 地址由平台固件分配，
                // 内核通常不重新映射 I/O BAR，这里只探测大小并记录。
                if raw & BAR_IO_BIT != 0 {
                    let size = bar_size32(0, dev, func, off);
                    pcie_log!(
                        "  BAR{} I/O    base={:#010x} size={:#x}",
                        bar,
                        raw & BAR_IO_ADDR_MASK,
                        size
                    );
                    bar += 1;
                    continue;
                }

                // --- Memory 空间 BAR ----------------------------------------------
                let mem_type = (raw >> BAR_MEM_TYPE_SHIFT) & BAR_MEM_TYPE_MASK;
                match mem_type {
                    BAR_MEM_TYPE_32BIT => {
                        // 32 位 Memory BAR：低 4 位为控制/属性位，不可寻址。
                        let size = bar_size32(0, dev, func, off);
                        if size == 0 {
                            bar += 1;
                            continue;
                        }
                        let base = alloc_pci_mmio(size as u64);
                        // 写入分配的基地址，保留原始低 4 位属性位。
                        let val = (base as u32) & BAR_MEM_ADDR_MASK | (raw & BAR_ATTR_MASK);
                        unsafe { cfg_write32(0, dev, func, off, val) };
                        pcie_log!("  BAR{} Mem32 base={:#010x} size={:#x}", bar, base, size);
                    }
                    BAR_MEM_TYPE_64BIT => {
                        // 64 位 Memory BAR：低 32 位在当前槽，高 32 位在下一个槽。
                        let next_off = off + PCI_BAR_SPACING;
                        let next_raw = unsafe { cfg_read32(0, dev, func, next_off) };

                        let size = bar_size32(0, dev, func, off);
                        if size == 0 {
                            bar += 2; // 跳过两个槽位
                            continue;
                        }
                        let base = alloc_pci_mmio(size as u64);

                        // 写低 32 位基地址，保留原始低 4 位属性位。
                        let low = (base as u32) & BAR_MEM_ADDR_MASK | (raw & BAR_ATTR_MASK);
                        unsafe { cfg_write32(0, dev, func, off, low) };

                        // 写高 32 位基地址。
                        let high = (base >> 32) as u32;
                        unsafe { cfg_write32(0, dev, func, next_off, high) };

                        pcie_log!(
                            "  BAR{} Mem64 base={:#010x} size={:#x} next_raw={:#010x}",
                            bar,
                            base,
                            size,
                            next_raw
                        );
                        bar += 1; // 额外跳过高 32 位占用的下一个槽位
                    }
                    _ => {
                        // 保留 / 不支持的 Memory 类型（01/11 为保留编码）。
                        pcie_log!(
                            "  BAR{} unknown mem_type={:#x} raw={:#010x}",
                            bar,
                            mem_type,
                            raw
                        );
                    }
                }
                bar += 1;
            }

            // 单功能设备（bit7 = 0），无需扫描后续功能
            if func == 0 && !header.is_multifunction() {
                break;
            }
        }
    }
}

// ─── 设备树探测入口 ──────────────────────────────────────────────────

/// PCIe 主机桥设备树探测回调
/// 完成 ECAM 空间注册和总线 0 设备扫描
fn pci_probe_callback(node: &DeviceNode, _compatible: &str) -> Result<(), &'static str> {
    pcie_log!("detect pcie host bridge");
    // 注册 ECAM 空间到系统 MMIO
    pcie_register_pcie_mmio_space(node);

    // 扫描总线 0 上的所有设备
    scan_bus_0();
    Ok(())
}

// 设备树探测入口：匹配 "pci-host-ecam-generic" 兼容字符串
dtb_probe! {
    compatible: "pci-host-ecam-generic",
    priority: High,
    driver: "pci-host",
    probe: pci_probe_callback
}
