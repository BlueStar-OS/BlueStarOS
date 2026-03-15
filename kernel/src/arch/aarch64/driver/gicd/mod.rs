//! aarch64 rk3588平台中断控制器
//!
//! GICD/GICR 基地址通过 dtb_probe 从设备树动态获取

pub mod mmio;
mod gicd;

// 导出 GIC 公共接口
pub use gicd::{gic_init, gic_read_iar, gic_write_eoir, gic_enable_spi, gic_enable_ppi};

/// UART2 SPI 中断号（orangepi5plus.dts: interrupts = <0x00 0x14d 0x04>）
/// SPI 333 → INTID = 333 + 32 = 365
pub const UART2_INTID: u32 = 365;

/// EL1 Physical Timer PPI INTID
pub const TIMER_PPI_INTID: u32 = 30;

const GICR_STRIDE: usize = 0x20000;

/// 获取 GICD 基地址（从设备树探测，回退到硬编码）
pub fn gicd_base() -> usize {
    let probed = mmio::gicd_base();
    if probed != 0 { probed } else { 0xfe60_0000 }
}

/// 获取 GICR 基地址（从设备树探测，回退到硬编码）
pub fn gicr_base() -> usize {
    let probed = mmio::gicr_base();
    if probed != 0 { probed } else { 0xfe68_0000 }
}

/// 当前 CPU 的 GICR RD_base
pub fn gicr_rd_base(cpu: usize) -> usize {
    gicr_base() + cpu * GICR_STRIDE
}

/// 当前 CPU 的 GICR SGI_base
pub fn gicr_sgi_base(cpu: usize) -> usize {
    gicr_base() + cpu * GICR_STRIDE + 0x10000
}