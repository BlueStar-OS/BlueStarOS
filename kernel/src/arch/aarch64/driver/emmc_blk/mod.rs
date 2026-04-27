//! eMMC 块设备驱动，通过 sdmmc crate 实现 BlockDevTrait trait

pub mod kernel_impl;
use crate::dtb::DeviceNode;
use crate::fs::vfs::register_global_block_device;
use crate::kprintln;
use crate::{
    dtb_probe,
    fs::vfs::{BlockDevTrait, VfsFsError},
};
use alloc::sync::Arc;
use log::info;
use sdmmc::emmc::EMmcHost;
use spin::Mutex;

/// QEMU virt aarch64 SDHCI 基地址（暂时硬编码，后续可通过 DTB 探测）
/// QEMU virt 的 SDHCI-PCI 设备 BAR0 映射地址
const QEMU_SDHCI_BASE: usize = 0x1000_0000;

pub static mut EMMC_DWCMSHC_ADDR: usize = 0;

/// eMMC 块设备，封装 sdmmc crate 的 EMmcHost
pub struct EmmcBlk {
    host: EMmcHost,
    capacity_sectors: u64,
}

unsafe impl Send for EmmcBlk {}
unsafe impl Sync for EmmcBlk {}

impl EmmcBlk {
    /// 初始化 eMMC 控制器并返回块设备实例
    pub fn new(base_addr: usize) -> Result<Self, VfsFsError> {
        let mut host = EMmcHost::new(base_addr);
        host.init().map_err(|e| {
            log::error!("eMMC init failed: {:?}", e);
            VfsFsError::IO
        })?;

        let cap = host.get_block_num();
        info!(
            "eMMC: capacity = {} sectors ({} MB)",
            cap,
            cap * 512 / 1024 / 1024
        );

        Ok(Self {
            host,
            capacity_sectors: cap,
        })
    }

    /// 使用默认 QEMU SDHCI 地址初始化
    pub fn new_qemu() -> Result<Self, VfsFsError> {
        Self::new(QEMU_SDHCI_BASE)
    }

    /// 使用 DTB probe 注册到的控制器地址初始化
    pub fn new_from_probe() -> Result<Self, VfsFsError> {
        let base_addr = discovered_emmc_base().ok_or(VfsFsError::IO)?;
        Self::new(base_addr)
    }
}

impl BlockDevTrait for EmmcBlk {
    fn read_block(&mut self, lba: usize, buf: &mut [u8]) -> Result<(), VfsFsError> {
        self.host
            .read_blocks(lba as u32, 1, buf)
            .map_err(|_| VfsFsError::IO)
    }

    fn write_block(&mut self, lba: usize, buf: &[u8]) -> Result<(), VfsFsError> {
        self.host
            .write_blocks(lba as u32, 1, buf)
            .map_err(|_| VfsFsError::IO)
    }

    fn capacity_in_sectors(&self) -> u64 {
        self.capacity_sectors
    }
}

pub fn emmc_callback(node: &DeviceNode, _compatible: &str) -> Result<(), &'static str> {
    // 获取寄存器地址
    let reg = node.get_property("reg").ok_or("Missing reg property")?;
    let regs = reg.as_reg(2, 2);
    if regs.is_empty() {
        return Err("Empty reg property");
    }

    let base_addr = regs[0].address as usize;
    kprintln!(
        "[RK3588 emmc Probe] Found Emmc at {:#x}, size={:#x}",
        base_addr,
        regs[0].size
    );

    unsafe {
        EMMC_DWCMSHC_ADDR = base_addr;
    }

    // 注册 eMMC MMIO 区域
    {
        use crate::arch::memory::VirAddr;
        use crate::memory::{register_kernel_mmio, MapAreaFlags, VirNumRange};

        let mmio_range = VirNumRange::new(
            VirAddr(base_addr),
            VirAddr(base_addr + (regs[0].size as usize) - 1),
        );
        let flags = MapAreaFlags::V
            | MapAreaFlags::R
            | MapAreaFlags::W
            | MapAreaFlags::A
            | MapAreaFlags::G
            | MapAreaFlags::DEV;
        register_kernel_mmio(mmio_range, flags);

        // 注册 CRU MMIO 区域 (时钟控制器)
        let cru_base = kernel_impl::RK3588_CRU_BASE;
        let cru_size = 0x5c000; // DTB: reg = <0xfd7c0000 0x5c000>
        let cru_range = VirNumRange::new(VirAddr(cru_base), VirAddr(cru_base + cru_size));
        register_kernel_mmio(cru_range, flags);
    }

    // 真机 eMMC 依赖时钟控制器，因此在 probe 阶段先完成时钟初始化，
    // 确认控制器可用后再统一注册到全局块设备表。
    kernel_impl::init_emmc_clk();
    let emmc = EmmcBlk::new(base_addr).map_err(|_| "eMMC init failed during DTB probe")?;
    register_global_block_device(Arc::new(Mutex::new(emmc)));

    Ok(())
}

// 注册 UART 探测器
crate::dtb_probe! {
    compatible: "rockchip,rk3588-dwcmshc",
    priority: Mid,
    driver: "rk3588-dwcmshc",
    probe: emmc_callback
}

pub fn discovered_emmc_base() -> Option<usize> {
    let base_addr = unsafe { EMMC_DWCMSHC_ADDR };
    (base_addr != 0).then_some(base_addr)
}
