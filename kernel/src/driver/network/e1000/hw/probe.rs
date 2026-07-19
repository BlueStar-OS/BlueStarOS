//! e1000 PCI probe 与硬件初始化流程。

use core::sync::atomic::Ordering;

use log::{debug, error, info, warn};

use crate::driver::network::e1000::hw::device::{E1000, E1000_DEV, E1000_MMIO_BASE};
use crate::driver::network::e1000::hw::irq::{e1000_intr_handler, e1000_irq_enable};
use crate::driver::network::e1000::hw::regs::{
    ReadMacRaw, E1000_CTRL, E1000_CTRL_RST, E1000_DEVICE_IDS, E1000_EERD, E1000_EERD_ADDR_SHIFT,
    E1000_EERD_DATA_SHIFT, E1000_EERD_DONE, E1000_EERD_START, E1000_ICR, E1000_IMC,
    E1000_NODE_ADDRESS_SIZE, E1000_RA, E1000_RAH_AV, E1000_RCTL, E1000_STATUS, E1000_STATUS_FD,
    E1000_STATUS_LU, E1000_TCTL, E1000_TCTL_PSP,
};
use crate::driver::network::e1000::queue::rx::{
    e1000_alloc_rx_buffers, e1000_configure_rx, e1000_setup_rx_resources, E1000RxRing,
    RX_RING_COUNT,
};
use crate::driver::network::e1000::queue::tx::{
    e1000_configure_tx, e1000_setup_link, e1000_setup_tx_resources, E1000TxRing, TX_RING_COUNT,
};
use crate::driver::pcie::pci_ids::PCI_VENDOR_ID_INTEL;
use crate::driver::pcie::{
    cfg_read16, cfg_write16, collect_pcie_devices_by_target, BarSpace, PcieBarInfo, PcieBarSpace,
    PcieDeviceInfo, PcieDeviceTarget, PCI_COMMAND, PCI_COMMAND_MASTER, PCI_COMMAND_MEMORY,
};
use crate::time::kernel_sleep;

/// e1000 设备筛选器。
pub struct E1000PcieDeviceTarget;

impl PcieDeviceTarget for E1000PcieDeviceTarget {
    fn matches(device: &PcieDeviceInfo) -> bool {
        device.vendor_id as u32 == PCI_VENDOR_ID_INTEL
            && E1000_DEVICE_IDS.contains(&device.device_id)
    }
}

/// e1000 硬件全局复位。
///
/// 参考 Linux: drivers/net/ethernet/intel/e1000/e1000_hw.c:376-514
fn e1000_reset_hw(bar: &BarSpace) {
    let flush = || {
        bar.read_32(E1000_STATUS);
    };

    bar.write_32(E1000_IMC, 0xFFFF_FFFF);
    flush();

    bar.write_32(E1000_RCTL, 0);
    bar.write_32(E1000_TCTL, E1000_TCTL_PSP);
    flush();

    kernel_sleep(10);

    let ctrl = bar.read_32(E1000_CTRL);
    info!("e1000: reset: issuing global reset (CTRL={:#010x})", ctrl);
    bar.write_32(E1000_CTRL, ctrl | E1000_CTRL_RST);
    flush();

    kernel_sleep(5);

    bar.write_32(E1000_IMC, 0xFFFF_FFFF);
    flush();
    let icr = bar.read_32(E1000_ICR);
    info!("e1000: reset: complete (ICR={:#010x})", icr);
}

/// 读取 MAC 地址，优先 RA[0]，回退 EEPROM。
///
/// 参考 Linux: drivers/net/ethernet/intel/e1000/e1000_hw.c:4229-4263
fn e1000_read_mac_addr(bar: &BarSpace) -> ReadMacRaw {
    let ral = bar.read_32(E1000_RA);
    let rah = bar.read_32(E1000_RA + 4);

    if (rah & E1000_RAH_AV) != 0 {
        info!(
            "e1000: MAC from ASIC (RA[0]): RAL={:#010x} RAH={:#010x} AV=1",
            ral, rah,
        );
        return ReadMacRaw { ral, rah };
    }

    warn!("e1000: RA[0] AV=0, falling back to EEPROM read via EERD");

    let mut raw_words = [0u16; 3];
    for (word_idx, word) in raw_words.iter_mut().enumerate() {
        let cmd = E1000_EERD_START | ((word_idx as u32) << E1000_EERD_ADDR_SHIFT);
        bar.write_32(E1000_EERD, cmd);

        loop {
            let eerd = bar.read_32(E1000_EERD);
            if (eerd & E1000_EERD_DONE) != 0 {
                *word = ((eerd >> E1000_EERD_DATA_SHIFT) & 0xFFFF) as u16;
                break;
            }
            core::hint::spin_loop();
        }
    }

    info!(
        "e1000: MAC from EEPROM: words={:#06x} {:#06x} {:#06x}",
        raw_words[0], raw_words[1], raw_words[2],
    );

    ReadMacRaw {
        ral: (raw_words[0] as u32) | ((raw_words[1] as u32) << 16),
        rah: (raw_words[2] as u32) | E1000_RAH_AV,
    }
}

/// 从 BAR 列表中找出 BAR0 MMIO。
fn find_e1000_bar0(bars: &[PcieBarInfo]) -> Option<&PcieBarInfo> {
    bars.iter().find(|bar| {
        bar.bar_index == 0 && matches!(bar.space, PcieBarSpace::Memory32 | PcieBarSpace::Memory64)
    })
}

/// 从全局 PCIe 注册表中探测并初始化 e1000。
pub fn probe_registered_e1000() {
    let e1000_devices = collect_pcie_devices_by_target::<E1000PcieDeviceTarget>();
    if e1000_devices.is_empty() {
        debug!("e1000: no device found");
        return;
    }

    for device in &e1000_devices {
        let bdf = device.bdf();
        info!(
            "e1000: found at {:02x}:{:02x}.{} vendor={:#06x} device={:#06x} class={:#06x}",
            bdf.0, bdf.1, bdf.2, device.vendor_id, device.device_id, device.class_code,
        );

        let old_command = unsafe { cfg_read16(bdf.0, bdf.1, bdf.2, PCI_COMMAND) };
        let new_command = old_command | PCI_COMMAND_MEMORY | PCI_COMMAND_MASTER;
        if new_command != old_command {
            unsafe { cfg_write16(bdf.0, bdf.1, bdf.2, PCI_COMMAND, new_command) };
            info!(
                "e1000: PCI command {:#06x} -> {:#06x} (MEM|MASTER)",
                old_command, new_command
            );
        } else {
            debug!("e1000: PCI command already {:#06x}", old_command);
        }

        debug!("e1000: BARs ({})", device.bars.len());
        for bar in &device.bars {
            debug!(
                "  BAR{}: offset={:#04x} space={:?} base={:#010x} size={:#x} prefetch={}",
                bar.bar_index,
                bar.config_offset,
                bar.space,
                bar.base_addr,
                bar.size,
                bar.is_prefetchable,
            );
        }

        let bar0 = match find_e1000_bar0(&device.bars) {
            Some(bar) => bar,
            None => {
                error!("e1000: no valid BAR0 found");
                continue;
            }
        };

        let bar0_addr = bar0.base_addr;
        let bar0_size = bar0.size;
        let bar0_space = bar0.build_bar_space();

        info!(
            "e1000: BAR0 MMIO [{:#x}..{:#x}) size={:#x}",
            bar0_addr,
            bar0_addr + bar0_size,
            bar0_size,
        );

        let status = bar0_space.read_32(E1000_STATUS);
        info!("e1000: STATUS register = {:#010x}", status);

        let link_up = (status & E1000_STATUS_LU) != 0;
        let full_duplex = (status & E1000_STATUS_FD) != 0;
        info!(
            "e1000: link={} duplex={}",
            if link_up { "UP" } else { "DOWN" },
            if full_duplex { "FULL" } else { "HALF" },
        );

        e1000_reset_hw(&bar0_space);
        e1000_setup_link(&bar0_space, full_duplex);

        let mac_raw = e1000_read_mac_addr(&bar0_space);
        let ral_le = mac_raw.ral.to_le_bytes();
        let rah_le = mac_raw.rah.to_le_bytes();
        let mac: [u8; E1000_NODE_ADDRESS_SIZE] = [
            ral_le[0], ral_le[1], ral_le[2], ral_le[3], rah_le[0], rah_le[1],
        ];
        info!(
            "e1000: MAC = {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
        );

        let mut tx_ring = E1000TxRing::new();
        e1000_setup_tx_resources(&mut tx_ring, TX_RING_COUNT);
        e1000_configure_tx(&bar0_space, &tx_ring);

        let mut rx_ring = E1000RxRing::new();
        e1000_setup_rx_resources(&mut rx_ring, RX_RING_COUNT);
        e1000_configure_rx(&bar0_space, &rx_ring);
        e1000_alloc_rx_buffers(&bar0_space, &mut rx_ring);
        // TODO:开网卡中断

        E1000_MMIO_BASE.store(bar0_addr as usize, Ordering::Release);
        e1000_irq_enable();

        let e1000 = E1000 {
            bar: bar0_space,
            rx_ring,
            tx_ring,
            mac,
        };
        E1000_DEV.lock(|dev| *dev = Some(e1000));

        info!("e1000: probe complete, driver ready");
    }
}
