use crate::arch::memory::{PhysiAddr, VirAddr};
use crate::driver::pcie::bar::*;
use crate::driver::pcie::pcie_helper::*;
use crate::dtb::DeviceNode;
use crate::register_kernel_mmio;
use crate::MapAreaFlags;
use crate::VirNumRange;
use crate::{dtb_probe, kprintln};
use core::fmt;
use log::error;
mod bar;
mod pcie_helper;

const PCI_VENDOR_ID: u16 = 0x00;
const PCI_DEVICE_ID: u16 = 0x02;
const PCI_COMMAND: u16 = 0x04;
const PCI_CLASS_REVISION: u16 = 0x08;
const PCI_HEADER_TYPE: u16 = 0x0e;
const PCI_BAR0: u16 = 0x10;

const PCI_COMMAND_IO: u16 = 0x1;
const PCI_COMMAND_MEMORY: u16 = 0x2;
// bus enable(dma)
const PCI_COMMAND_MASTER: u16 = 0x4;

static mut PCIE_ECAM_ADDR: usize = 0;
///log helper
#[macro_export]
macro_rules! pcie_log {
    ($fmt: literal $(, $($arg: tt)+)?) => {
        $crate::console::kprint(format_args!(concat!("[PCIE]: ",$fmt, "\n") $(, $($arg)+)?))
    }
}

/// pcie enable device
pub fn pci_enable_device(bus: u8, dev: u8, func: u8) {
    let mut cmd = unsafe { cfg_read16(bus, dev, func, PCI_COMMAND) };
    cmd |= PCI_COMMAND_MEMORY | PCI_COMMAND_MASTER;
    unsafe { cfg_write16(bus, dev, func, PCI_COMMAND, cmd) };

    let new_cmd = unsafe { cfg_read16(bus, dev, func, PCI_COMMAND) };
    if new_cmd != cmd {
        error!(
            "[PCIE]: commnad set error ,expect command {:#x} real_command:{:#x} ",
            cmd, new_cmd
        );
        return;
    }
    pcie_log!("PCI command: {:#x} -> {:#x}", cmd, new_cmd);
}

/// pcie disable device
pub fn pci_disable_device(bus: u8, dev: u8, func: u8) {
    let mut cmd = unsafe { cfg_read16(bus, dev, func, PCI_COMMAND) };
    cmd &= !(PCI_COMMAND_MEMORY | PCI_COMMAND_MASTER);
    unsafe { cfg_write16(bus, dev, func, PCI_COMMAND, cmd) };

    let new_cmd = unsafe { cfg_read16(bus, dev, func, PCI_COMMAND) };
    if new_cmd != cmd {
        error!(
            "[PCIE]: commnad disable error ,expect command {:#x} real_command:{:#x} ",
            cmd, new_cmd
        );
        return;
    }
    pcie_log!("PCI command: {:#x} -> {:#x}", cmd, new_cmd);
}

///register pcie mmio space
fn pcie_register_pcie_mmio_space(node: &DeviceNode) {
    pcie_log!("Start register MMIO memory");
    // 获取寄存器地址

    if let Ok(reg) = node.get_property("reg").ok_or("Missing reg property") {
        let regs = reg.as_reg(2, 2);
        if regs.is_empty() {
            error!("[PCIE] host regs is empty");
            return;
        }

        // 注册ecam MMIO 区域
        for reg in regs {
            pcie_log!("registe one mmio regen");
            let base_addr = reg.address as usize;
            let mmio_range = VirNumRange::new(
                VirAddr(base_addr),
                VirAddr(base_addr + (reg.size as usize) - 1),
            );
            let flags = MapAreaFlags::V
                | MapAreaFlags::R
                | MapAreaFlags::W
                | MapAreaFlags::A
                | MapAreaFlags::G
                | MapAreaFlags::DEV;
            register_kernel_mmio(mmio_range, flags);

            // update pcie ecam addr
            unsafe {
                PCIE_ECAM_ADDR = reg.address as usize;
            }
        }
    } else {
        error!("[PCIE] node get property fail!");
    }

    // register pcie window mmio range
    pcie_log!("register pcie window mmio range");
    let pcie_window_mmio_range = VirNumRange::new(VirAddr(0x40000000), VirAddr(0x7fffffff));

    let flags = MapAreaFlags::V
        | MapAreaFlags::R
        | MapAreaFlags::W
        | MapAreaFlags::A
        | MapAreaFlags::G
        | MapAreaFlags::DEV;
    register_kernel_mmio(pcie_window_mmio_range, flags);
}

fn scan_bus_0() {
    for dev in 0..device_per_bus {
        for func in 0..function_per_device {
            let vendor = unsafe { cfg_read16(0, dev, func, PCI_VENDOR_ID) };
            if vendor == 0xffff {
                if func == 0 {
                    break;
                }
                continue;
            }

            let device = unsafe { cfg_read16(0, dev, func, PCI_DEVICE_ID) };
            let class_rev = unsafe { cfg_read32(0, dev, func, PCI_CLASS_REVISION) };
            let class_code = class_rev >> 8;
            let header = unsafe { cfg_read16(0, dev, func, PCI_HEADER_TYPE) as u8 };

            pcie_log!(
                "PCI 00:{:02x}.{} vendor={:04x} device={:04x} class={:06x} header={:02x}",
                dev,
                func,
                vendor,
                device,
                class_code,
                header
            );

            for bar in 0..6u16 {
                let off = PCI_BAR0 + bar * 4;
                let raw = unsafe { cfg_read32(0, dev, func, off) };
                // TODO: now only scan bus 0
                let bar_size = bar_size32(0, dev, func, off);

                //  not support 64 size
                let bar_memory_type = decode_bar(raw, None);

                let alloc_addr = alloc_pci_mmio(bar_size as u64);

                assign_bar0(0, dev, func);

                pcie_log!("  BAR{} raw={:#010x}", bar, raw);
            }

            if func == 0 && (header & 0x80) == 0 {
                break;
            }
        }
    }
}

///pcie host detect callback function
fn pci_probe_callback(node: &DeviceNode, _compatible: &str) -> Result<(), &'static str> {
    pcie_log!("detect pcie host bridge");
    // register ecam space to sys mmio
    pcie_register_pcie_mmio_space(node);

    // scan all device of bus 0
    scan_bus_0();
    Ok(())
}

// pcie detect
dtb_probe! {
    compatible: "pci-host-ecam-generic",
    priority: High,
    driver: "pci-host",
    probe: pci_probe_callback
}
