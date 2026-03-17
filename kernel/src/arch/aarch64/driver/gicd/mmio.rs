//! 根据设备树探测 GICD / GICR 基地址并恒等映射到内核地址空间
//! 探测结果存入全局静态变量，供 GIC 驱动和内核页表初始化使用

use crate::driver::dtb::DeviceNode;
use crate::kprintln;
/// GICD 基地址和大小（探测后填充）
static mut GICD_BASE_ADDR: usize = 0;
static mut GICD_SIZE: usize = 0;

/// GICR 基地址和大小（探测后填充）
static mut GICR_BASE_ADDR: usize = 0;
static mut GICR_SIZE: usize = 0;

/// 获取探测到的 GICD 基地址（0 表示未探测到）
pub fn gicd_base() -> usize {
    unsafe { GICD_BASE_ADDR }
}

/// 获取探测到的 GICD 区域大小
pub fn gicd_size() -> usize {
    unsafe { GICD_SIZE }
}

/// 获取探测到的 GICR 基地址（0 表示未探测到）
pub fn gicr_base() -> usize {
    unsafe { GICR_BASE_ADDR }
}

/// 获取探测到的 GICR 区域大小
pub fn gicr_size() -> usize {
    unsafe { GICR_SIZE }
}

/// GICv3 设备树探测回调
///
/// orangepi5plus.dts 中的 GIC 节点：
/// ```dts
/// interrupt-controller@fe600000 {
///     compatible = "arm,gic-v3";
///     reg = <0x00 0xfe600000 0x00 0x10000   /* GICD */
///            0x00 0xfe680000 0x00 0x100000>; /* GICR */
/// };
/// ```
///
/// reg 属性有两组：第一组是 GICD，第二组是 GICR
/// address-cells=2, size-cells=2（根节点定义）
fn gicv3_probe(node: &DeviceNode, _compatible: &str) -> Result<(), &'static str> {
    let reg = node.get_property("reg").ok_or("GICv3: missing reg")?;
    let regs = reg.as_reg(2, 2);

    if regs.len() < 2 {
        return Err("GICv3: reg 属性需要至少两组（GICD + GICR）");
    }

    let gicd_addr = regs[0].address as usize;
    let gicd_sz = regs[0].size as usize;
    let gicr_addr = regs[1].address as usize;
    let gicr_sz = regs[1].size as usize;

    kprintln!("[GICv3 Probe] GICD: {:#x} size={:#x}", gicd_addr, gicd_sz);
    kprintln!("[GICv3 Probe] GICR: {:#x} size={:#x}", gicr_addr, gicr_sz);

    unsafe {
        GICD_BASE_ADDR = gicd_addr;
        GICD_SIZE = gicd_sz;
        GICR_BASE_ADDR = gicr_addr;
        GICR_SIZE = gicr_sz;
    }

    // 注册 GIC MMIO 区域到内核内存空间列表
    {
        use crate::arch::memory::VirAddr;
        use crate::memory::{register_kernel_mmio, MapAreaFlags, VirNumRange};

        let gic_start = gicd_addr;
        let gic_end = gicr_addr + gicr_sz;
        let gic_range = VirNumRange::new(VirAddr(gic_start), VirAddr(gic_end));
        let flags = MapAreaFlags::V
            | MapAreaFlags::R
            | MapAreaFlags::W
            | MapAreaFlags::A
            | MapAreaFlags::G
            | MapAreaFlags::DEV;
        register_kernel_mmio(gic_range, flags);
    }

    Ok(())
}

// 注册 GICv3 探测器（High 优先级，中断控制器必须最先初始化）
crate::dtb_probe! {
    compatible: "arm,gic-v3",
    priority: High,
    driver: "gicv3",
    probe: gicv3_probe
}
