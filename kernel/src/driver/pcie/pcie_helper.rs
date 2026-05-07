use crate::driver::pcie::PhysiAddr;
use crate::driver::pcie::PCIE_ECAM_ADDR;
use log::error;

// ─── ECAM 地址编码参数 ───────────────────────────────────────────────

/// 总线号在 ECAM 地址中的位移位（bit20 起）
pub const BUS_SHIFT: u32 = 20;
/// 设备号在 ECAM 地址中的位移位（bit15 起）
pub const DEV_SHIFT: u32 = 15;
/// 功能号在 ECAM 地址中的位移位（bit12 起）
pub const FUNC_SHIFT: u32 = 12;

// ─── 总线扫描参数 ────────────────────────────────────────────────────

/// 每条 PCI 总线最多 32 个设备
pub const DEVICE_PER_BUS: u8 = 32;
/// 每个设备最多 8 个功能
pub const FUNCTION_PER_DEVICE: u8 = 8;

/// 每个功能的 ECAM 配置空间为 4KB
pub const FUNC_ECAM_SIZE: u32 = 4096;

// ─── 配置空间访问辅助常量 ────────────────────────────────────────────

/// 32 位字内的字节偏移掩码（用于 16 位非对齐访问定位）
const BYTE_OFFSET_MASK: u16 = 0x3;
/// 32 位对齐掩码（清除低 2 位）
const ALIGN32_MASK: u16 = !0x3u16;
/// 字节偏移转位偏移的移位量（1 字节 = 8 位）
const BYTE_TO_BIT_SHIFT: u32 = 3;
/// 16 位读取掩码
const U16_MASK: u32 = 0xFFFF;

// ─── ECAM 地址计算 ──────────────────────────────────────────────────

/// 根据总线号、设备号、功能号和寄存器偏移计算 ECAM 物理地址
fn get_ecam_addr_of_bus(bus_no: u8, dev_no: u8, func_no: u8, offset: u16) -> PhysiAddr {
    let ecam_base_addr = unsafe { PCIE_ECAM_ADDR };
    if ecam_base_addr == 0 {
        error!("[PCIE]: ECAM base address is zero!");
        return PhysiAddr(0);
    }
    // ECAM 地址 = 基址 + (bus << 20) + (dev << 15) + (func << 12) + offset
    let addr: usize = ecam_base_addr
        + ((bus_no as usize) << BUS_SHIFT)
        + ((dev_no as usize) << DEV_SHIFT)
        + ((func_no as usize) << FUNC_SHIFT)
        + offset as usize;
    PhysiAddr(addr)
}

// ─── 配置空间读写 ────────────────────────────────────────────────────

/// 从 PCIe 配置空间读取 32 位值
///
/// # Safety
/// 调用者需保证 bus/dev/func/offset 参数有效，且设备确实存在
pub unsafe fn cfg_read32(bus: u8, dev: u8, func: u8, offset: u16) -> u32 {
    let addr = get_ecam_addr_of_bus(bus, dev, func, offset);
    core::ptr::read_volatile(addr.0 as *const u32)
}

/// 从 PCIe 配置空间读取 16 位值
///
/// 16 位寄存器可能不在 32 位边界上，此函数先做 32 位对齐读取，
/// 再按字节偏移提取目标 16 位。
///
/// # Safety
/// 调用者需保证 bus/dev/func/offset 参数有效，且设备确实存在
pub unsafe fn cfg_read16(bus: u8, dev: u8, func: u8, offset: u16) -> u16 {
    // 先按 32 位对齐读，再根据偏移提取对应 16 位
    let aligned = cfg_read32(bus, dev, func, offset & ALIGN32_MASK);
    let shift = (offset & BYTE_OFFSET_MASK) as u32 * BYTE_TO_BIT_SHIFT;
    ((aligned >> shift) & U16_MASK) as u16
}

/// 向 PCIe 配置空间写入 32 位值
///
/// # Safety
/// 调用者需保证 bus/dev/func/offset 参数有效，且设备确实存在
pub unsafe fn cfg_write32(bus: u8, dev: u8, func: u8, offset: u16, val: u32) {
    let addr = get_ecam_addr_of_bus(bus, dev, func, offset);
    core::ptr::write_volatile(addr.0 as *mut u32, val);
}

/// 向 PCIe 配置空间写入 16 位值
///
/// # Safety
/// 调用者需保证 bus/dev/func/offset 参数有效，且设备确实存在
pub unsafe fn cfg_write16(bus: u8, dev: u8, func: u8, offset: u16, val: u16) {
    let addr = get_ecam_addr_of_bus(bus, dev, func, offset);
    core::ptr::write_volatile(addr.0 as *mut u16, val);
}
