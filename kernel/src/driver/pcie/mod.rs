use crate::arch::memory::{PhysiAddr, VirAddr};
use crate::driver::pcie::bar::*;
use crate::driver::pcie::pci_ids::*;
use crate::driver::pcie::pcie_helper::*;
use crate::dtb::DeviceNode;
use crate::dtb_probe;
use crate::error::BlueErr;
use crate::register_kernel_mmio;
use crate::sync::UPSafeCell;
use crate::MapAreaFlags;
use crate::VirNumRange;
use alloc::vec::Vec;
use core::fmt;
use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicU8, Ordering};
use lazy_static::lazy_static;
use log::{debug, error, info, warn};

mod bar;
pub mod pci_ids;
mod pcie_helper;

pub use pcie_helper::*;

/// PCIe 设备筛选 trait。
///
/// 使用场景：
/// 1. 上层驱动可以为“自己关心的设备”定义一个匹配器；
/// 2. 然后通过 `collect_pcie_devices_by_target::<T>()` 直接收集目标设备；
/// 3. 避免每个驱动都手写一遍 vendor/device/class 的遍历逻辑。
pub trait PcieDeviceTarget {
    /// 判断一个已枚举设备是否属于当前目标类型。
    fn matches(device: &PcieDeviceInfo) -> bool;
}

/// PCI BAR 的地址空间类型。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PcieBarSpace {
    /// I/O port BAR。
    Io,
    /// 32 位 memory BAR。
    Memory32,
    /// 64 位 memory BAR。
    Memory64,
}

/// 单个 PCI BAR 的采集结果。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PcieBarInfo {
    /// 第几个 BAR 槽位，取值范围通常是 0..=5。
    pub bar_index: u8,
    /// 该 BAR 在配置空间中的寄存器偏移。
    pub config_offset: u16,
    /// BAR 对应的地址空间类型。
    pub space: PcieBarSpace,
    /// 当前 BAR 的基地址。
    pub base_addr: u64,
    /// 当前 BAR 探测出的大小。
    pub size: u64,
    /// memory BAR 是否可预取；I/O BAR 固定为 false。
    pub is_prefetchable: bool,
}

///左开右闭
#[derive(Default)]
pub struct BarSpace(PhysiAddr, PhysiAddr);

impl BarSpace {
    pub fn start_addr(&self) -> PhysiAddr {
        self.0
    }
    pub fn end_addr(&self) -> PhysiAddr {
        self.1
    }
    pub fn read_16(&self, offset: usize) -> u16 {
        // 检查对齐
        if offset & (0x1) != 0 {
            error!("Unalign read!");
            return 0;
        }
        let addr = self.0 .0 + offset;

        // 检查范围
        if addr >= self.1 .0 {
            error!(" out of bar space ");
            return 0;
        }
        unsafe {
            let value = read_volatile(addr as *const u16);
            return value;
        }
    }
    pub fn write_16(&self, offset: usize, value: u16) {
        // 检查对齐
        if offset & (0x1) != 0 {
            error!("Unalign write!");
            return;
        }
        let addr = self.0 .0 + offset;

        // 检查范围
        if addr >= self.1 .0 {
            error!(" out of bar space ");
            return;
        }
        unsafe {
            write_volatile(addr as *mut u16, value);
        }
    }
    pub fn read_32(&self, offset: usize) -> u32 {
        // 检查对齐
        if offset & (0x3) != 0 {
            error!("Unalign read!");
            return 0;
        }
        let addr = self.0 .0 + offset;

        // 检查范围
        if addr >= self.1 .0 {
            error!(" out of bar space ");
            return 0;
        }
        unsafe {
            let value = read_volatile(addr as *const u32);
            return value;
        }
    }
    pub fn write_32(&self, offset: usize, value: u32) {
        // 检查对齐
        if offset & (0x3) != 0 {
            error!("Unalign write!");
            return;
        }
        let addr = self.0 .0 + offset;

        // 检查范围
        if addr >= self.1 .0 {
            error!(" out of bar space ");
            return;
        }
        unsafe {
            write_volatile(addr as *mut u32, value);
        }
    }
    pub fn read_64(&self, offset: usize) -> u64 {
        // 检查对齐
        if offset & (0x7) != 0 {
            error!("Unalign read!");
            return 0;
        }
        let addr = self.0 .0 + offset;

        // 检查范围
        if addr >= self.1 .0 {
            error!(" out of bar space ");
            return 0;
        }
        unsafe {
            let value = read_volatile(addr as *const u64);
            return value;
        }
    }
    pub fn write_64(&self, offset: usize, value: u64) {
        // 检查对齐
        if offset & (0x7) != 0 {
            error!("Unalign write!");
            return;
        }
        let addr = self.0 .0 + offset;

        // 检查范围
        if addr >= self.1 .0 {
            error!(" out of bar space ");
            return;
        }
        unsafe {
            write_volatile(addr as *mut u64, value);
        }
    }
}

// 建立bar视图
impl PcieBarInfo {
    pub fn build_bar_space(&self) -> BarSpace {
        let start = self.base_addr;
        let end = self.base_addr + self.size;
        BarSpace(PhysiAddr(start as usize), PhysiAddr(end as usize))
    }
}

/// 一个 PCIe 功能（BDF）在扫描后的完整描述。
///
/// 设计说明：
/// 1. `bars` 保存该功能所有已实现 BAR；
/// 2. `base_addr` 额外缓存“第一个 memory BAR 的基地址”，方便后续驱动快速拿 MMIO 基址；
/// 3. 全局注册表按“功能”粒度保存，而不是按“设备号”粒度保存，因为 multifunction device
///    的不同 function 可能是完全不同的逻辑设备。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PcieDeviceInfo {
    /// 总线号。
    pub bus_number: u8,
    /// 设备号。
    pub device_number: u8,
    /// 功能号。
    pub function_number: u8,
    /// Vendor ID。
    pub vendor_id: u16,
    /// Device ID。
    pub device_id: u16,
    /// 24 位 class code（高 16 位 class/subclass，低 8 位 prog-if）。
    pub class_code: u32,
    /// Header Type 的布局值（去掉 multifunction bit 后的低 7 位）。
    pub header_type: u8,
    /// 是否多功能设备。
    pub is_multifunction: bool,
    /// 第一个 memory BAR 的基地址，供后续 MMIO 驱动快速使用。
    pub base_addr: Option<PhysiAddr>,
    /// 该功能的全部 BAR 信息快照。
    pub bars: Vec<PcieBarInfo>,
}

impl PcieDeviceInfo {
    /// 返回当前设备的 BDF 三元组。
    pub fn bdf(&self) -> (u8, u8, u8) {
        (self.bus_number, self.device_number, self.function_number)
    }
}

lazy_static! {
    /// 全局 PCIe 设备注册表。
    ///
    /// 扫描流程在枚举到每个 BDF 后立即注册，后续显卡、网卡、块设备等驱动
    /// 只需要消费这里的快照，不需要再次直接扫配置空间。
    pub static ref PCIE_DEVICES: UPSafeCell<Vec<PcieDeviceInfo>> =
        UPSafeCell::new(Vec::new());
}

/// 注册一个已扫描到的 PCIe 设备。
///
/// 如果同一个 BDF 被重复注册，则使用新结果覆盖旧结果，避免重扫后保留旧快照。
pub fn register_pcie_device(device_info: PcieDeviceInfo) {
    let mut devices = PCIE_DEVICES.lock();
    if let Some(existing_device) = devices.iter_mut().find(|existing_device| {
        existing_device.bus_number == device_info.bus_number
            && existing_device.device_number == device_info.device_number
            && existing_device.function_number == device_info.function_number
    }) {
        *existing_device = device_info;
        return;
    }

    devices.push(device_info);
}

/// 清空全局 PCIe 设备注册表。
///
/// 每次从根总线重新扫描前都应调用，避免旧的枚举结果残留。
pub fn clear_pcie_devices() {
    PCIE_DEVICES.lock().clear();
}

/// 获取当前全部 PCIe 设备的快照。
///
/// 返回副本而不是直接暴露全局表，避免调用方长期持有全局锁。
pub fn collect_pcie_devices() -> Vec<PcieDeviceInfo> {
    PCIE_DEVICES.lock().iter().cloned().collect()
}

/// 按目标筛选器收集 PCIe 设备。
pub fn collect_pcie_devices_by_target<Target: PcieDeviceTarget>() -> Vec<PcieDeviceInfo> {
    PCIE_DEVICES
        .lock()
        .iter()
        .filter(|device_info| Target::matches(device_info))
        .cloned()
        .collect()
}

/// 按 vendor/device id 查找第一个匹配设备。
pub fn find_pcie_device(vendor_id: u16, device_id: u16) -> Option<PcieDeviceInfo> {
    PCIE_DEVICES
        .lock()
        .iter()
        .find(|device_info| {
            device_info.vendor_id == vendor_id && device_info.device_id == device_id
        })
        .cloned()
}

/// 按 BDF 精确查找一个 PCIe 功能。
pub fn find_pcie_device_by_bdf(
    bus_number: u8,
    device_number: u8,
    function_number: u8,
) -> Option<PcieDeviceInfo> {
    PCIE_DEVICES
        .lock()
        .iter()
        .find(|device_info| {
            device_info.bus_number == bus_number
                && device_info.device_number == device_number
                && device_info.function_number == function_number
        })
        .cloned()
}

// ─── 总线号 newtype ───────────────────────────────────────────────────

/// PCI 总线编号（0–255）
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct BusNo(u8);

impl BusNo {
    pub const ROOT: Self = Self(0);

    pub fn new(bus_no: u8) -> Self {
        Self(bus_no)
    }

    /// 取出总线号的原始 `u8` 值。
    pub fn value(self) -> u8 {
        self.0
    }
}

/// 全局总线号分配器，从 1 开始（bus 0 是根总线）
static NEXT_ALLOC_BUSNO: AtomicU8 = AtomicU8::new(1);

/// 布局代码：CardBus bridge（Type 2）
pub const PCI_HEADER_TYPE_CARDBUS: u8 = 2;

// ─── PCI 配置空间标准寄存器偏移 ─────────────────────────────────────

/// 厂商 ID（16 位）
pub const PCI_VENDOR_ID: u16 = 0x00;
/// 设备 ID（16 位）
pub const PCI_DEVICE_ID: u16 = 0x02;
/// 命令寄存器（16 位）
pub const PCI_COMMAND: u16 = 0x04;
/// 类别码 / 版本号（32 位，[31:16] class，[15:0] revision）
pub const PCI_CLASS_REVISION: u16 = 0x08;
/// 头类型寄存器（8 位，bit7 = 多功能标志，bits[6:0] = 布局代码）
pub const PCI_HEADER_TYPE: u16 = 0x0E;
/// BAR0 基地址寄存器
pub const PCI_BAR0: u16 = 0x10;
/// BAR 寄存器间距（字节）
pub const PCI_BAR_SPACING: u16 = 4;
/// 桥总线号寄存器（Primary / Secondary / Subordinate）
const PCI_BRIDGE_BUS_NUMBER: u16 = 0x18;
/// Type 1 bridge memory base 寄存器。
const PCI_MEMORY_BASE: u16 = 0x20;
/// Type 1 bridge memory limit 寄存器。
const PCI_MEMORY_LIMIT: u16 = 0x22;
/// Type 1 bridge 普通 memory window 的最小粒度为 1 MiB。
///
/// 参考 Linux 5.4.29 `drivers/pci/probe.c:448-453`
const PCI_BRIDGE_MEMORY_WINDOW_GRANULARITY: u64 = 0x10_0000;

// ─── PCI 命令寄存器位定义 ────────────────────────────────────────────

/// 使能 I/O 空间访问
pub const PCI_COMMAND_IO: u16 = 0x1;
/// 使能内存空间访问
pub const PCI_COMMAND_MEMORY: u16 = 0x2;
/// 使能总线主控（DMA）
pub const PCI_COMMAND_MASTER: u16 = 0x4;

// ─── PCI 中断相关 ────────────────────────────────────────────────────

/// PCI Interrupt Pin 寄存器偏移 (byte at offset 0x3D)
/// 1=INTA, 2=INTB, 3=INTC, 4=INTD
pub const PCI_INTERRUPT_PIN: u16 = 0x3D;

/// QEMU riscv64 virt: PCIE IRQ 基地址 (PCIE_IRQ = 0x20, IRQ 32-35)
/// 参考 QEMU include/hw/riscv/virt.h:72
pub const PCIE_IRQ_BASE: u32 = 32;
/// QEMU riscv64 virt: PCIE IRQ 数量 (GPEX_NUM_IRQS = 4)
pub const PCIE_IRQ_COUNT: u32 = 4;

// ─── PCI 特殊值 ──────────────────────────────────────────────────────

/// 无效厂商 ID：该槽位无设备
const PCI_INVALID_VENDOR: u16 = 0xFFFF;
/// 类别码在 CLASS_REVISION 中的位移位
const PCI_CLASS_CODE_SHIFT: u32 = 8;

// ─── PCI 头类型 ──────────────────────────────────────────────────────

/// 多功能标志位掩码（bit7）
const PCI_MULTIFUNCTION_BIT: u8 = 0x80;
/// 布局代码掩码（bits[6:0]）
const PCI_HEADER_TYPE_MASK: u8 = 0x7F;

/// PCI 头类型寄存器 16 位对齐掩码（清除 bit0，确保 16 位对齐读取）
const PCI_HEADER_TYPE_ALIGN: u16 = !0x1;

/// 布局代码：标准端点（Type 0）
const PCI_HEADER_TYPE_ENDPOINT: u8 = 0x00;
/// 布局代码：PCI-to-PCI 桥（Type 1）
const PCI_HEADER_TYPE_BRIDGE: u8 = 0x01;

/// Type 0（端点）Bar 数量
const BAR_COUNT_ENDPOINT: u16 = 6;
/// Type 1（桥）Bar 数量
const BAR_COUNT_BRIDGE: u16 = 2;

/// PCI Header Type 寄存器（偏移 0x0E）的封装。
///
/// 寄存器布局：
///   bit7    — 多功能标志
///   bits[6:0] — 布局代码（0x00 = 端点，0x01 = 桥，0x02 = CardBus）
#[derive(Clone, Copy, Debug)]
struct HeaderType(u8);

impl HeaderType {
    fn from_raw(raw: u8) -> Self {
        Self(raw)
    }

    /// 布局代码（bits 6:0）
    fn layout(self) -> u8 {
        self.0 & PCI_HEADER_TYPE_MASK
    }

    /// 设备是否为多功能设备
    fn is_multifunction(self) -> bool {
        self.0 & PCI_MULTIFUNCTION_BIT != 0
    }

    /// 当前布局是否属于“会继续向下挂总线”的桥。
    fn is_bridge_layout(self) -> bool {
        matches!(
            self.layout(),
            PCI_HEADER_TYPE_BRIDGE | PCI_HEADER_TYPE_CARDBUS
        )
    }

    /// 该布局支持的 Bar 数量
    fn bar_count(self) -> u16 {
        match self.layout() {
            PCI_HEADER_TYPE_BRIDGE => BAR_COUNT_BRIDGE,
            PCI_HEADER_TYPE_ENDPOINT => BAR_COUNT_ENDPOINT,
            _ => 0,
        }
    }
}

impl fmt::LowerHex for HeaderType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::LowerHex::fmt(&self.0, f)
    }
}

// ─── PCIe MMIO 窗口 ──────────────────────────────────────────────────

/// PCI MMIO 窗口基地址（1GB 窗口）
const PCIE_WINDOW_MMIO_BASE: usize = 0x4000_0000;
/// PCI MMIO 窗口结束地址
const PCIE_WINDOW_MMIO_END: usize = 0x7FFF_FFFF;

// ─── 已知设备 ID ─────────────────────────────────────────────────────

/// QEMU edu 教学设备：厂商 ID
const PCI_EDU_VENDOR: u16 = 0x1234;
/// QEMU edu 教学设备：设备 ID
const PCI_EDU_DEVICE: u16 = 0x11e8;

/// 判断当前 BDF 是否为 QEMU edu 教学设备。
fn is_qemu_edu_device(vendor_id: u16, device_id: u16) -> bool {
    vendor_id == PCI_EDU_VENDOR && device_id == PCI_EDU_DEVICE
}

// ─── 全局状态 ────────────────────────────────────────────────────────

/// ECAM 基地址，设备树探测后填入
static mut PCIE_ECAM_ADDR: usize = 0;

// ─── 日志宏 ──────────────────────────────────────────────────────────

#[macro_export]
macro_rules! pcie_log {
    ($fmt: literal $(, $($arg: tt)+)?) => {
        $crate::console::kprint(format_args!(concat!("[PCIE]: ",$fmt, "\n") $(, $($arg)+)?))
    }
}

// ─── 设备命令 ────────────────────────────────────────────────────────

/// 按调用方给出的目标命令字回写 `PCI_COMMAND`。
///
/// 这里不自己决定开哪些 bit，而是让上层在 BAR sizing、bridge forwarding 等不同阶段
/// 显式传入需要的命令字，避免“这里偷偷开了某个 decode，调用方并不知道”。
pub fn pci_enable_device(bus: u8, dev: u8, func: u8, command_value: u16) {
    unsafe { cfg_write16(bus, dev, func, PCI_COMMAND, command_value) };

    let new_cmd = unsafe { cfg_read16(bus, dev, func, PCI_COMMAND) };
    if new_cmd != command_value {
        error!(
            "[PCIE]: command set error, expect {:#x} real {:#x}",
            command_value, new_cmd
        );
        return;
    }
    // TODO(dirinkbottle): 如果后续要做更细粒度的设备初始化，
    // 可以把 command bit 的打开拆成“按资源类型逐步打开”。
}

/// 暂时关闭一个设备的 I/O / Memory decode，并返回关闭前的原始命令字。
///
/// 当前主要给 BAR sizing 用：在把 BAR 临时写成 `0xffff_ffff` 之前先关 decode，
/// 避免设备在 sizing 期间错误响应总线事务。
pub fn pci_disable_device(bus: u8, dev: u8, func: u8) -> u16 {
    let mut command_value = unsafe { cfg_read16(bus, dev, func, PCI_COMMAND) };
    let old_command_value = command_value;
    command_value &= !(PCI_COMMAND_MEMORY | PCI_COMMAND_IO);
    unsafe { cfg_write16(bus, dev, func, PCI_COMMAND, command_value) };

    let new_cmd = unsafe { cfg_read16(bus, dev, func, PCI_COMMAND) };
    if new_cmd != command_value {
        error!(
            "[PCIE]: command disable error, expect {:#x} real {:#x}",
            command_value, new_cmd
        );
    }
    old_command_value
    //pcie_log!("PCI command: {:#x} -> {:#x}", old_cmd, new_cmd);
}

// ─── MMIO 空间注册 ──────────────────────────────────────────────────

/// 注册 ECAM 地址空间和 BAR MMIO 窗口
fn pcie_register_pcie_mmio_space(node: &DeviceNode) {
    pcie_log!("Start register MMIO memory");

    if let Ok(reg) = node.get_property("reg").ok_or("Missing reg property") {
        let regs = reg.as_reg(2, 2);
        if regs.is_empty() {
            error!("[PCIE] host regs is empty");
            return;
        }

        for reg in regs {
            pcie_log!("register one mmio region");
            let base = reg.address as usize;
            let range = VirNumRange::new(VirAddr(base), VirAddr(base + (reg.size as usize) - 1));
            let flags = MapAreaFlags::V
                | MapAreaFlags::R
                | MapAreaFlags::W
                | MapAreaFlags::A
                | MapAreaFlags::G
                | MapAreaFlags::DEV;
            register_kernel_mmio(range, flags);

            unsafe {
                PCIE_ECAM_ADDR = reg.address as usize;
            }
        }
    } else {
        error!("[PCIE] node get property fail!");
    }

    // 注册 BAR 映射窗口
    pcie_log!("register pcie window mmio range");
    let window_range = VirNumRange::new(
        VirAddr(PCIE_WINDOW_MMIO_BASE),
        VirAddr(PCIE_WINDOW_MMIO_END),
    );
    let flags = MapAreaFlags::V
        | MapAreaFlags::R
        | MapAreaFlags::W
        | MapAreaFlags::A
        | MapAreaFlags::G
        | MapAreaFlags::DEV;
    register_kernel_mmio(window_range, flags);
}

// ─── BAR 探测 ────────────────────────────────────────────────────────

/// 探测并编程单个设备的 BAR
///
/// 对设备配置空间的 BAR0–BAR5（或桥的 BAR0–BAR1）做 sizing，
/// 为 Memory BAR 分配 MMIO 地址空间并回写基地址。
fn probe_device_bars(
    bus: u8,
    dev: u8,
    func: u8,
    header: HeaderType,
    class_16: u16,
) -> Vec<PcieBarInfo> {
    let mut probed_bars = Vec::new();

    // 根据 class 和 header 判断是否应跳过 BAR 探测
    let (max_bars, skip) = bar_scan_policy(header, class_16);

    if skip {
        pcie_log!(
            "  skip BAR probe (class={:#06x}, header={:02x})",
            class_16,
            header
        );
        return probed_bars;
    }

    let mut bar: u16 = 0;
    while bar < max_bars {
        let off = PCI_BAR0 + bar * PCI_BAR_SPACING;
        let raw = unsafe { cfg_read32(bus, dev, func, off) };

        // 参考:
        //   1. drivers/pci/probe.c:pci_setup_device() 1792-1798, 1845-1854
        //      Linux 先按 header type 决定“应该扫描几个 BAR 槽位”，
        //      Type 0 扫 6 个，Type 1 扫 2 个，然后无条件进入 pci_read_bases()。
        //   2. drivers/pci/probe.c:pci_read_bases() 320-335
        //      Linux 对每个 BAR 槽位都调用 __pci_read_base()，并不会因为当前配置值 l/raw 为 0 就提前跳过。
        //   3. drivers/pci/probe.c:__pci_read_base() 196-199, 251-258
        //      Linux 的“BAR 是否实现”判断依据是 size probe 的返回值 sz/sz64，
        //      不是 BAR 当前地址值 l/raw。当前值为 0 只说明“还没分配地址”，不等于“BAR 不存在”。
        //
        // 当前代码这里的 if raw == 0 提前 continue，会把“已实现但尚未分配地址”的 BAR 直接漏掉。
        // 你现在遇到的 VGA 正是这一类：BAR0 被你分到了 0x40000000，但 BAR2 这种 control/MMIO BAR
        // 可能初始值就是 0；如果这里先 continue，后面的 sizing 就永远不会发生。
        //
        // 最小修复步骤建议按 Linux 的顺序做:
        //   Step 1. 先读 raw，只把它当作“类型/属性来源”，不要当作“是否存在”的判据。
        //   Step 2. 无条件对这个 bar_off 做 size probe，得到 size。
        //   Step 3. 只有 size == 0 时才认定“BAR 未实现”，然后跳过。
        //   Step 4. 如果 raw 指示它是 64-bit BAR，还要连同高 32 位一起处理，并额外跳过下一个槽位。
        //   Step 5. 只有在确认 size != 0 之后，才进入分配地址并回写 BAR 的阶段。
        //
        // 你真正要改的不是“如何从 raw 取类型”，而是“是否继续”的判据：
        //   错误判据: raw == 0
        //   正确判据: size == 0

        let Some(bar_kind) = BarKind::from_raw(raw) else {
            pcie_log!("  BAR{} unknown raw={:#010x}", bar, raw);
            bar += 1;
            continue;
        };

        match bar_kind {
            BarKind::Io => {
                let (bar_size_bytes, original_command) = bar_size(bus, dev, func, off, bar_kind);
                if bar_size_bytes == 0 {
                    pci_enable_device(bus, dev, func, original_command);
                    bar += 1;
                    continue;
                }

                pcie_log!(
                    "  BAR{} I/O    base={:#010x} size={:#x}",
                    bar,
                    raw & BAR_IO_ADDR_MASK,
                    bar_size_bytes
                );
                // sizing 结束后恢复原命令字，并确保打开 I/O decode。
                pci_enable_device(
                    bus,
                    dev,
                    func,
                    original_command | bar_kind.command_decode_bit(),
                );

                probed_bars.push(PcieBarInfo {
                    bar_index: bar as u8,
                    config_offset: off,
                    space: PcieBarSpace::Io,
                    base_addr: (raw & BAR_IO_ADDR_MASK) as u64,
                    size: bar_size_bytes,
                    is_prefetchable: false,
                });
                bar += 1;
                continue;
            }
            BarKind::Mem32 => {
                let (bar_size_bytes, original_command) = bar_size(bus, dev, func, off, bar_kind);
                if bar_size_bytes == 0 {
                    pci_enable_device(bus, dev, func, original_command);
                    bar += 1;
                    continue;
                }

                let base = alloc_pci_mmio(bar_size_bytes);
                let val = (base as u32) & BAR_MEM_ADDR_MASK | (raw & BAR_ATTR_MASK);
                unsafe {
                    cfg_write32(bus, dev, func, off, val);
                }
                // BAR 回写完成后恢复旧命令字，并确保打开 memory decode。
                pci_enable_device(
                    bus,
                    dev,
                    func,
                    original_command | bar_kind.command_decode_bit(),
                );
                pcie_log!(
                    "  BAR{} Mem32 base={:#010x} size={:#x}",
                    bar,
                    base,
                    bar_size_bytes
                );
                probed_bars.push(PcieBarInfo {
                    bar_index: bar as u8,
                    config_offset: off,
                    space: PcieBarSpace::Memory32,
                    base_addr: base,
                    size: bar_size_bytes,
                    is_prefetchable: (raw & BAR_PREFETCH_BIT) != 0,
                });
            }
            BarKind::Mem64 => {
                let next_off = off + PCI_BAR_SPACING;
                let next_raw = unsafe { cfg_read32(bus, dev, func, next_off) };
                let (bar_size_bytes, original_command) = bar_size(bus, dev, func, off, bar_kind);

                if bar_size_bytes == 0 {
                    pci_enable_device(bus, dev, func, original_command);
                    bar += 2;
                    continue;
                }

                let base = alloc_pci_mmio(bar_size_bytes);
                let low = (base as u32) & BAR_MEM_ADDR_MASK | (raw & BAR_ATTR_MASK);
                unsafe {
                    cfg_write32(bus, dev, func, off, low);
                }
                let high = (base >> 32) as u32;
                unsafe {
                    cfg_write32(bus, dev, func, next_off, high);
                }
                // 64 位 BAR 要在高低 32 位都回写完之后再恢复 decode。
                pci_enable_device(
                    bus,
                    dev,
                    func,
                    original_command | bar_kind.command_decode_bit(),
                );
                pcie_log!(
                    "  BAR{} Mem64 base={:#010x} size={:#x} next_raw={:#010x}",
                    bar,
                    base,
                    bar_size_bytes,
                    next_raw
                );
                probed_bars.push(PcieBarInfo {
                    bar_index: bar as u8,
                    config_offset: off,
                    space: PcieBarSpace::Memory64,
                    base_addr: base,
                    size: bar_size_bytes,
                    is_prefetchable: (raw & BAR_PREFETCH_BIT) != 0,
                });
                bar += 1; // 跳过被高 32 位占用的槽位
            }
        }
        bar += 1;
    }

    probed_bars
}

/// 根据 class 和 header type 决定 BAR 扫描策略
///
/// 返回 `(max_bars, skip)`：
/// - host bridge（class 0x0600）：BAR 不可探测，跳过
/// - PCI bridge（class 0x0604）但不是 type 1 header：class/header 不匹配，跳过
/// - 正常设备：按 header type 返回 BAR 数量
fn bar_scan_policy(header: HeaderType, class_16: u16) -> (u16, bool) {
    if class_16 == PCI_CLASS_BRIDGE_HOST as u16 {
        return (0, true);
    }
    if class_16 == PCI_CLASS_BRIDGE_PCI as u16 && header.layout() != PCI_HEADER_TYPE_BRIDGE {
        warn!(
            "PCI class/header mismatch: class={:#06x} header={:02x}",
            class_16, header
        );
        return (0, true);
    }
    (header.bar_count(), false)
}

// ─── 总线扫描 ────────────────────────────────────────────────────────

/// 分配一个新的总线号。
fn alloc_busno() -> BusNo {
    let no = NEXT_ALLOC_BUSNO.load(Ordering::SeqCst);
    NEXT_ALLOC_BUSNO.store(no + 1, Ordering::SeqCst);
    BusNo(no)
}

/// 写 PCI-PCI 桥的总线号寄存器（offset 0x18）
///
/// 寄存器布局：
/// ```text
/// 31       24 23       16 15        8 7         0
/// +----------+----------+----------+----------+
/// | Sec.Lat  | Subord.  | Secondary| Primary  |
/// +----------+----------+----------+----------+
/// ```
fn write_bridge_bus_numbers(
    bus: u8,
    dev: u8,
    func: u8,
    primary: BusNo,
    secondary: BusNo,
    subordinate: BusNo,
) {
    let val: u32 = (subordinate.0 as u32) << 16 | (secondary.0 as u32) << 8 | primary.0 as u32;
    unsafe {
        cfg_write32(bus, dev, func, PCI_BRIDGE_BUS_NUMBER, val);
    }
}

/// 向下对齐到指定粒度。
fn align_down(value: u64, align: u64) -> u64 {
    value & !(align - 1)
}

/// 向上对齐到指定粒度。
fn align_up(value: u64, align: u64) -> u64 {
    (value + align - 1) & !(align - 1)
}

/// PCI 普通 memory window base/limit 地址掩码 (bits [15:4], bits [3:0] 保留)
///
/// 参考 PCI-to-PCI Bridge Architecture Spec:
/// `PCI_MEMORY_BASE` 和 `PCI_MEMORY_LIMIT` 寄存器编码 addr[31:20] 于 bits[15:4]，
/// bits[3:0] 保留应填 0。
/// 参考 Linux 5.4.29 `include/uapi/linux/pci_regs.h`: PCI_MEMORY_RANGE_MASK
pub const PCI_MEMORY_WINDOW_ADDR_MASK: u16 = 0xFFF0;

/// 当前阶段使用的 Type 1 bridge 普通 memory window。
///
/// 设计边界：
/// 1. 只对应 `PCI_MEMORY_BASE/LIMIT` 这一组普通 memory window 寄存器；
/// 2. 只能表达 32 位地址空间里的 1 MiB 粒度区间；
/// 3. 当前为了先打通 QEMU VGA，会把子树里所有 memory BAR 暂时合并到这一组窗口里。
///
/// 参考 Linux 5.4.29：
/// - `drivers/pci/setup-bus.c:609-626 pci_setup_bridge_mmio()`
/// - `drivers/pci/probe.c:437-457 pci_read_bridge_mmio()`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BridgeMemoryWindow32 {
    /// 对齐后的窗口起始地址。
    start_addr: u32,
    /// 对齐后的窗口结束地址（包含末地址）。
    end_addr_inclusive: u32,
}

impl BridgeMemoryWindow32 {
    /// 从子树设备实际使用的原始地址范围构造桥窗口。
    fn from_device_span(raw_start_addr: u64, raw_end_addr_inclusive: u64) -> Option<Self> {
        let aligned_start_addr = align_down(raw_start_addr, PCI_BRIDGE_MEMORY_WINDOW_GRANULARITY);
        let aligned_end_addr_exclusive = align_up(
            raw_end_addr_inclusive.checked_add(1)?,
            PCI_BRIDGE_MEMORY_WINDOW_GRANULARITY,
        );
        let aligned_end_addr_inclusive = aligned_end_addr_exclusive - 1;

        if aligned_start_addr > u32::MAX as u64 || aligned_end_addr_inclusive > u32::MAX as u64 {
            return None;
        }

        Some(Self {
            start_addr: aligned_start_addr as u32,
            end_addr_inclusive: aligned_end_addr_inclusive as u32,
        })
    }

    /// 编码 `PCI_MEMORY_BASE` 低 16 位字段。
    fn encoded_base_field(self) -> u16 {
        ((self.start_addr >> 16) as u16) & PCI_MEMORY_WINDOW_ADDR_MASK
    }

    /// 编码 `PCI_MEMORY_LIMIT` 高 16 位字段。
    fn encoded_limit_field(self) -> u16 {
        ((self.end_addr_inclusive >> 16) as u16) & PCI_MEMORY_WINDOW_ADDR_MASK
    }

    /// 编码为 `cfg_write32(..., PCI_MEMORY_BASE, value)` 需要写入的 32 位值。
    fn encoded_register_dword(self) -> u32 {
        ((self.encoded_limit_field() as u32) << 16) | self.encoded_base_field() as u32
    }
}

/// 当前阶段桥 memory window 收集结果。
///
/// 之所以不用 `Option<BridgeMemoryWindow32>`，是因为“没有资源”和“超出当前实现能力”
/// 是两种完全不同的情况，后续调试时需要明确区分。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BridgeMemoryWindowCollection {
    /// 子树里没有任何已分配的 memory BAR。
    Empty,
    /// 当前阶段可直接写进 `PCI_MEMORY_BASE/LIMIT` 的 32 位窗口。
    Window32(BridgeMemoryWindow32),
    /// 子树 memory BAR 跨到了 4 GiB 以上，超出当前“普通 32 位 bridge window”的实现能力。
    Unsupported64BitRange {
        raw_start_addr: u64,
        raw_end_addr_inclusive: u64,
    },
}

/// 从一个设备的 BAR 列表中提取“第一个 memory BAR”作为快速访问基址。
///
/// 这样后续显卡、网卡等驱动如果只关心 MMIO base，就不需要每次自己遍历 `bars`。
fn first_memory_bar_base_addr(probed_bars: &[PcieBarInfo]) -> Option<PhysiAddr> {
    probed_bars
        .iter()
        .find(|bar_info| {
            bar_info.space == PcieBarSpace::Memory32 || bar_info.space == PcieBarSpace::Memory64
        })
        .map(|bar_info| PhysiAddr(bar_info.base_addr as usize))
}

/// 根据一次枚举结果构造全局注册表中的设备条目。
///
/// 参考 Linux 5.4 的设备发现顺序：
/// 1. `pci_setup_device()` 先读取 header/class/BAR 等基础信息；
/// 2. 后续桥扫描和具体驱动绑定都消费这份“已经解析好的设备描述”。
/// 参考文件：
///   - `/home/inkbottle/桌面/linux-5.4.29/drivers/pci/probe.c:1792-1854`
///   - `/home/inkbottle/桌面/linux-5.4.29/drivers/pci/probe.c:320-335`
fn build_pcie_device_info(
    bus: BusNo,
    dev: u8,
    func: u8,
    vendor_id: u16,
    device_id: u16,
    class_code: u32,
    header: HeaderType,
    probed_bars: Vec<PcieBarInfo>,
) -> PcieDeviceInfo {
    let base_addr = first_memory_bar_base_addr(&probed_bars);

    PcieDeviceInfo {
        bus_number: bus.0,
        device_number: dev,
        function_number: func,
        vendor_id,
        device_id,
        class_code,
        header_type: header.layout(),
        is_multifunction: header.is_multifunction(),
        base_addr,
        bars: probed_bars,
    }
}

/// 收集当前阶段需要写进 Type 1 bridge 的普通 memory window。
///
/// 当前行为：
/// 1. 只根据 `secondary..=subordinate` 子树里的已分配 BAR 反推地址区间；
/// 2. 暂时不区分 prefetchable / non-prefetchable；
/// 3. 如果区间超过 32 位地址能力，则交给上层打日志并保留 TODO。
///
/// TODO(dirinkbottle):
/// 1. 按 `is_prefetchable` 拆成普通 memory window 和 PREF window；
/// 2. PREF window 需要补 `PCI_PREF_MEMORY_*` 和 `UPPER32`；
/// 3. 如果将来出现 non-prefetchable BAR 被分到 4 GiB 以上，需要重新约束 BAR 分配策略。
fn collect_stage1_bridge_memory_window(
    secondary_bus: BusNo,
    subordinate_bus: BusNo,
) -> BridgeMemoryWindowCollection {
    let devices = PCIE_DEVICES.lock();
    let mut raw_window_start_addr = u64::MAX;
    let mut raw_window_end_addr_inclusive = 0u64;

    for device_info in devices.iter().filter(|device_info| {
        device_info.bus_number >= secondary_bus.value()
            && device_info.bus_number <= subordinate_bus.value()
    }) {
        for bar_info in device_info.bars.iter().filter(|bar_info| {
            bar_info.size != 0
                && matches!(
                    bar_info.space,
                    PcieBarSpace::Memory32 | PcieBarSpace::Memory64
                )
        }) {
            raw_window_start_addr = raw_window_start_addr.min(bar_info.base_addr);
            raw_window_end_addr_inclusive =
                raw_window_end_addr_inclusive.max(bar_info.base_addr + bar_info.size - 1);
        }
    }

    if raw_window_start_addr == u64::MAX {
        return BridgeMemoryWindowCollection::Empty;
    }

    match BridgeMemoryWindow32::from_device_span(
        raw_window_start_addr,
        raw_window_end_addr_inclusive,
    ) {
        Some(bridge_memory_window) => BridgeMemoryWindowCollection::Window32(bridge_memory_window),
        None => BridgeMemoryWindowCollection::Unsupported64BitRange {
            raw_start_addr: raw_window_start_addr,
            raw_end_addr_inclusive: raw_window_end_addr_inclusive,
        },
    }
}

/// 把当前阶段收集到的 32 位普通 memory window 编码进桥配置空间。
///
/// 参考 Linux 5.4.29 `drivers/pci/setup-bus.c:615-625`
fn program_bridge_memory_window32(
    bus: u8,
    dev: u8,
    func: u8,
    bridge_memory_window: BridgeMemoryWindow32,
) {
    let expected_base_field = bridge_memory_window.encoded_base_field();
    let expected_limit_field = bridge_memory_window.encoded_limit_field();
    let expected_register_dword = bridge_memory_window.encoded_register_dword();

    unsafe {
        cfg_write32(bus, dev, func, PCI_MEMORY_BASE, expected_register_dword);
    }

    let actual_base_field = unsafe { cfg_read16(bus, dev, func, PCI_MEMORY_BASE) };
    let actual_limit_field = unsafe { cfg_read16(bus, dev, func, PCI_MEMORY_LIMIT) };
    if actual_base_field != expected_base_field || actual_limit_field != expected_limit_field {
        error!(
            "[PCIE]: bridge memory window verify failed expect base={:#x} limit={:#x} real base={:#x} limit={:#x}",
            expected_base_field,
            expected_limit_field,
            actual_base_field,
            actual_limit_field
        );
    }
}

/// 当前阶段的桥 forwarding 初始化。
///
/// 当前只做最小可用实现：
/// 1. 写普通 `PCI_MEMORY_BASE/LIMIT`；
/// 2. 打开桥的 `PCI_COMMAND_MEMORY`；
/// 3. 让 CPU 到下游 memory BAR 的事务可以穿过 bridge。
///
/// 参考：
/// - Linux 5.4.29 `drivers/pci/setup-bus.c:609-626`
/// - QEMU `hw/pci/pci_bridge.c:193-206`
///
/// TODO(dirinkbottle):
/// 1. 空窗口时，按 Linux 一样把桥编码成 disabled window；
/// 2. 继续补 prefetchable window；
/// 3. 继续补 I/O window；
/// 4. 把 command bit 的打开也细分成 memory / io 两类。
fn setup_stage1_bridge_forwarding(
    bridge_bus: BusNo,
    bridge_device_number: u8,
    bridge_function_number: u8,
    secondary_bus: BusNo,
    subordinate_bus: BusNo,
) {
    match collect_stage1_bridge_memory_window(secondary_bus, subordinate_bus) {
        BridgeMemoryWindowCollection::Empty => {
            pcie_log!(
                "  bridge memory window: child-bus={}..{} empty",
                secondary_bus.value(),
                subordinate_bus.value()
            );
        }
        BridgeMemoryWindowCollection::Window32(bridge_memory_window) => {
            program_bridge_memory_window32(
                bridge_bus.value(),
                bridge_device_number,
                bridge_function_number,
                bridge_memory_window,
            );

            let original_command = unsafe {
                cfg_read16(
                    bridge_bus.value(),
                    bridge_device_number,
                    bridge_function_number,
                    PCI_COMMAND,
                )
            };
            pci_enable_device(
                bridge_bus.value(),
                bridge_device_number,
                bridge_function_number,
                original_command | PCI_COMMAND_MEMORY,
            );

            pcie_log!(
                "  bridge memory window: child-bus={}..{} range=[{:#x}-{:#x}]",
                secondary_bus.value(),
                subordinate_bus.value(),
                bridge_memory_window.start_addr,
                bridge_memory_window.end_addr_inclusive
            );
        }
        BridgeMemoryWindowCollection::Unsupported64BitRange {
            raw_start_addr,
            raw_end_addr_inclusive,
        } => {
            warn!(
                "[PCIE]: bridge child-bus={}..{} memory span [{:#x}-{:#x}] exceeds current 32-bit window implementation",
                secondary_bus.value(),
                subordinate_bus.value(),
                raw_start_addr,
                raw_end_addr_inclusive
            );
        }
    }
}

/// 递归扫描一条总线上的所有设备，发现 PCI-PCI 桥时递归进入子总线
///
/// 返回该子树中最大的总线号（即 subordinate bus number）。
/// 参考 Linux 5.4 `pci_scan_child_bus()` → `pci_scan_bridge_extend()`。
fn scan_bus_recursive(bus: BusNo) -> Result<BusNo, BlueErr> {
    let mut subordinate = bus;

    for dev in 0..DEVICE_PER_BUS {
        for func in 0..FUNCTION_PER_DEVICE {
            let vendor = unsafe { cfg_read16(bus.0, dev, func, PCI_VENDOR_ID) };
            if vendor == PCI_INVALID_VENDOR {
                if func == 0 {
                    break;
                }
                continue;
            }

            let device = unsafe { cfg_read16(bus.0, dev, func, PCI_DEVICE_ID) };
            let class_rev = unsafe { cfg_read32(bus.0, dev, func, PCI_CLASS_REVISION) };
            let class_code = class_rev >> PCI_CLASS_CODE_SHIFT;
            let class_16 = (class_code >> 8) as u16;
            // 这里必须对齐读
            let header = HeaderType::from_raw(unsafe {
                cfg_read16(bus.0, dev, func, PCI_HEADER_TYPE & PCI_HEADER_TYPE_ALIGN) as u8
            });

            pcie_log!(
                "PCI {:02x}:{:02x}.{} vendor={:x} device={:04x} class={:06x} header={:02x}",
                bus.0,
                dev,
                func,
                vendor,
                device,
                class_code,
                header
            );

            if is_qemu_edu_device(vendor, device) {
                pcie_log!("Find edu device at {:02x}:{:02x}.{}", bus.0, dev, func);
            }

            // 参考 Linux 5.4 pci_is_bridge() pci.h:633-636:
            // 只看 header type，不看 class
            let is_bridge = header.is_bridge_layout();
            let mut probed_bars = Vec::new();

            if is_bridge {
                let secondary = alloc_busno();
                // 先设 subordinate = secondary，递归后会更新为真实值
                write_bridge_bus_numbers(bus.0, dev, func, bus, secondary, secondary);

                let max_sub = scan_bus_recursive(secondary)?;
                subordinate = subordinate.max(max_sub);

                // 用递归返回的真实值更新 subordinate
                write_bridge_bus_numbers(bus.0, dev, func, bus, secondary, subordinate);

                setup_stage1_bridge_forwarding(bus, dev, func, secondary, subordinate);

                pcie_log!(
                    "  bridge: primary={}, secondary={}, subordinate={}",
                    bus.0,
                    secondary.0,
                    subordinate.0
                );
            } else if header.layout() == PCI_HEADER_TYPE_ENDPOINT
                && class_16 == PCI_CLASS_BRIDGE_PCI as u16
            {
                // 参考 Linux 5.4 pci_setup_device() probe.c:1792-1795:
                // header=0x00 但 class=0x0604，class/header 矛盾，拒绝设备
                error!(
                    "[PCIE]: ignoring class {:06x} (doesn't match header type {:02x})",
                    class_code, header
                );
                if func == 0 && !header.is_multifunction() {
                    break;
                }
                continue;
            } else {
                // 非桥设备：探测 BAR
                probed_bars = probe_device_bars(bus.0, dev, func, header, class_16);
            }

            // 把当前 BDF 的枚举快照注册到全局表，供后续驱动直接消费。
            register_pcie_device(build_pcie_device_info(
                bus,
                dev,
                func,
                vendor,
                device,
                class_code,
                header,
                probed_bars,
            ));

            // 单功能设备，跳过剩余功能号
            if func == 0 && !header.is_multifunction() {
                break;
            }
        }
    }

    Ok(subordinate)
}

// ─── 设备树探测入口 ──────────────────────────────────────────────────

fn pci_probe_callback(node: &DeviceNode, _compatible: &str) -> Result<(), &'static str> {
    pcie_log!("detect pcie host bridge");
    pcie_register_pcie_mmio_space(node);

    clear_pcie_devices();
    NEXT_ALLOC_BUSNO.store(1, Ordering::SeqCst);
    let _ = scan_bus_recursive(BusNo::ROOT);

    // PCIe 扫描完成后再交给 NVMe 驱动消费 BDF 快照。
    //
    // 原因：
    // 1. 这里已经完成了整棵 PCIe 总线扫描；
    // 2. `PCIE_DEVICES` 此时包含了所有可消费的 BDF 快照；
    // 3. NVMe 驱动应该消费这份快照，而不是重新直扫 ECAM；
    // 4. TODO(dirinkbottle): NVMe probe 成功后直接 `register_global_block_device()`，
    //    后面的 `RootFs::init_rootfs()` 就能无缝看到它。
    let _ = crate::driver::nvme::probe_registered_pcie_nvme_devices();

    // e1000探测
    let _ = crate::driver::network::e1000::probe_registered_e1000();

    Ok(())
}

dtb_probe! {
    compatible: "pci-host-ecam-generic",
    priority: High,
    driver: "pci-host",
    probe: pci_probe_callback
}
