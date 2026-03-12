// AArch64 SBI-like interface
// AArch64没有SBI，但我们提供相同的接口以保持架构抽象层的一致性

use core::arch::asm;
use log::error;
use crate::fs::vfs::{ROOTFS, VfsFs};

/// 关机操作（包含文件系统卸载）
pub fn shutdown() -> ! {
    // 取消文件系统挂载
    if let Some(rootfs) = ROOTFS.try_lock() {
        rootfs.as_ref().unwrap().mount_poinr.iter().for_each(|fs| {
            fs.1.lock().umount();
        });
    }

    // AArch64: 使用PSCI (Power State Coordination Interface) 关机
    // PSCI_SYSTEM_OFF = 0x84000008
    unsafe {
        asm!(
            "movz x0, #0x0008",           // 低16位
            "movk x0, #0x8400, lsl #16",  // 高16位
            "hvc #0",                      // Hypervisor call
            options(noreturn)
        );
    }
}

/// 设置下一次的时钟中断
pub fn set_next_timetriger(timer: usize) {
    // AArch64: 使用通用定时器 (Generic Timer)
    // 设置 CNTP_CVAL_EL0 (物理定时器比较值)
    unsafe {
        asm!(
            "msr cntp_cval_el0, {}",
            in(reg) timer as u64
        );
    }
}
