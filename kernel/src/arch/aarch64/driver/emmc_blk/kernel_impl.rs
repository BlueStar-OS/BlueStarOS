//! sdmmc crate 平台适配：Kernel trait（延时）+ Clk trait（RK3588 CRU 时钟）

extern crate alloc;
use alloc::boxed::Box;

use sdmmc::Kernel;
use sdmmc::emmc::clock::{Clk, ClkError, init_global_clk};
use rk3588_clk::{Rk3588Cru, constant::CCLK_EMMC};
use core::ptr::NonNull;

/// 内核延时实现，使用 aarch64 通用计时器自旋等待
struct BlueKernel;

impl Kernel for BlueKernel {
    fn sleep(us: u64) {
        let freq = crate::arch::time::read_time_freq() as u64;
        let ticks = freq * us / 1_000_000;
        let target = crate::arch::time::read_time() as u64 + ticks;
        while (crate::arch::time::read_time() as u64) < target {
            core::hint::spin_loop();
        }
    }
}

sdmmc::set_impl!(BlueKernel);

/// RK3588 CRU 基地址
pub const RK3588_CRU_BASE: usize = 0xfd7c0000;

/// 用 Rk3588Cru 实现真正的时钟控制
struct Rk3588ClkUnit {
    cru: Rk3588Cru,
}

unsafe impl Send for Rk3588ClkUnit {}
unsafe impl Sync for Rk3588ClkUnit {}

impl Clk for Rk3588ClkUnit {
    fn emmc_get_clk(&self) -> Result<u64, ClkError> {
        self.cru.mmc_get_clk(CCLK_EMMC)
            .map(|r| r as u64)
            .map_err(|_| ClkError::InvalidClockRate)
    }

    fn emmc_set_clk(&self, rate: u64) -> Result<u64, ClkError> {
        self.cru.mmc_set_clk(CCLK_EMMC, rate as usize)
            .map(|r| r as u64)
            .map_err(|_| ClkError::InvalidClockRate)
    }
}

/// 初始化全局时钟，必须在 EMmcHost::init() 之前调用
pub fn init_emmc_clk() {
    let ptr = NonNull::new(RK3588_CRU_BASE as *mut u8)
        .expect("CRU base address is null");
    let cru = Rk3588Cru::new(ptr);

    let unit = Rk3588ClkUnit { cru };
    let static_clk: &'static dyn Clk = Box::leak(Box::new(unit));
    init_global_clk(static_clk);
}
