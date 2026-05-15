//! e1000 中断处理与中断屏蔽控制。

use core::sync::atomic::Ordering;

use log::{info, warn};

use crate::driver::network::e1000::hw::device::{E1000_MMIO_BASE, E1000_RX_INTR_PENDING};
use crate::driver::network::e1000::hw::regs::{
    E1000_ICR, E1000_ICR_LSC, E1000_ICR_RXDMT0, E1000_ICR_RXO, E1000_ICR_RXT0, E1000_IMC,
    E1000_IMS, E1000_IMS_ENABLE_MASK, E1000_STATUS, E1000_STATUS_LU,
};

/// e1000 中断处理函数。
///
/// 参考 Linux: drivers/net/ethernet/intel/e1000/e1000_main.c:3741-3784
pub fn e1000_intr_handler() {
    let mmio_base = E1000_MMIO_BASE.load(Ordering::Relaxed);
    if mmio_base == 0 {
        return;
    }

    // SAFETY: `mmio_base` 在 probe 成功后固定为 BAR0 MMIO 有效地址。
    let icr = unsafe { (mmio_base as *const u32).add(E1000_ICR / 4).read_volatile() };
    if icr == 0 {
        return;
    }

    if (icr & (E1000_ICR_RXT0 | E1000_ICR_RXO | E1000_ICR_RXDMT0)) != 0 {
        // SAFETY: `mmio_base` 指向有效 BAR0 MMIO。
        unsafe {
            (mmio_base as *mut u32)
                .add(E1000_IMC / 4)
                .write_volatile(0xFFFF_FFFF);
        }
        E1000_RX_INTR_PENDING.store(true, Ordering::Release);
    }

    if (icr & E1000_ICR_LSC) != 0 {
        // SAFETY: `mmio_base` 指向有效 BAR0 MMIO。
        let status = unsafe {
            (mmio_base as *const u32)
                .add(E1000_STATUS / 4)
                .read_volatile()
        };
        if (status & E1000_STATUS_LU) != 0 {
            info!("e1000: link up");
        } else {
            warn!("e1000: link down");
        }
    }
}

/// 启用 e1000 中断。
pub fn e1000_irq_enable() {
    let mmio_base = E1000_MMIO_BASE.load(Ordering::Relaxed);
    if mmio_base == 0 {
        return;
    }
    // SAFETY: `mmio_base` 指向有效 BAR0 MMIO。
    unsafe {
        (mmio_base as *mut u32)
            .add(E1000_IMS / 4)
            .write_volatile(E1000_IMS_ENABLE_MASK);
    }
}

/// 禁用 e1000 中断并清除未决中断。
pub fn e1000_irq_disable() {
    let mmio_base = E1000_MMIO_BASE.load(Ordering::Relaxed);
    if mmio_base == 0 {
        return;
    }
    // SAFETY: `mmio_base` 指向有效 BAR0 MMIO。
    unsafe {
        (mmio_base as *mut u32)
            .add(E1000_IMC / 4)
            .write_volatile(0xFFFF_FFFF);
        (mmio_base as *const u32).add(E1000_ICR / 4).read_volatile();
    }
}
