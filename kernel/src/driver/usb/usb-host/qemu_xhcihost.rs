//! QEMU PCI implementation of the common xHCI host protocol.

use super::xhci_host::{register_xhci_host, xhci_irq_handler, AsUsbHost, XhciHost};
#[cfg(feature = "riscv64-qemu")]
use crate::arch::riscv64::driver::plic::{debug_irq_state, register_irq, QEMU_XHCI_IRQ};
use crate::driver::pcie::cfg_read16;
use crate::driver::pcie::{
    collect_pcie_devices_by_target, pci_enable_device, BarSpace, PcieBarSpace, PcieDeviceInfo,
    PcieDeviceTarget, PCI_COMMAND, PCI_COMMAND_MASTER, PCI_COMMAND_MEMORY,
};
use log::{error, info};

const QEMU_VENDOR_ID: u16 = 0x1b36;
const QEMU_XHCI_DEVICE_ID: u16 = 0x000d;

/// QEMU-specific platform object. It is the only adapter needed by xHCI:
/// identity plus BAR-relative MMIO access.
struct QemuXhciPlatform {
    bar: BarSpace,
}

impl AsUsbHost for QemuXhciPlatform {
    fn name(&self) -> &'static str {
        "qemu-xhci"
    }

    fn read32(&self, offset: usize) -> u32 {
        self.bar.read_32(offset)
    }

    fn write32(&self, offset: usize, value: u32) {
        self.bar.write_32(offset, value);
    }

    fn write64(&self, offset: usize, value: u64) {
        self.bar.write_64(offset, value);
    }
}

/// Select the PCI function documented by QEMU as `qemu-xhci`.
pub struct QemuXhciPcieTarget;

impl PcieDeviceTarget for QemuXhciPcieTarget {
    fn matches(device: &PcieDeviceInfo) -> bool {
        device.vendor_id == QEMU_VENDOR_ID
            && device.device_id == QEMU_XHCI_DEVICE_ID
            && device.class_code == crate::driver::pcie::pci_ids::PCI_CLASS_SERIAL_USB_XHCI
    }
}

/// The QEMU host is the common protocol host with QEMU MMIO.
pub type QemuXhciHost = XhciHost;

fn new_qemu_xhci_host(bar: BarSpace) -> Result<QemuXhciHost, &'static str> {
    XhciHost::new(QemuXhciPlatform { bar })
}

/// Probe QEMU xHCI functions after the common PCI scan assigned BARs.
pub fn probe_registered_qemu_xhcihost() {
    let devices = collect_pcie_devices_by_target::<QemuXhciPcieTarget>();
    if devices.is_empty() {
        info!("xhci: no QEMU xHCI controller found");
        return;
    }

    // xHCI is behind rp5 (root-port slot 6). Its INTA is swizzled to
    // root-complex IRQ 2, which is PLIC source 0x20 + 2 = IRQ 34.
    #[cfg(feature = "riscv64-qemu")]
    register_irq(QEMU_XHCI_IRQ, xhci_irq_handler, 1);

    for device in devices {
        let bdf = device.bdf();
        let Some(bar) = device.bars.iter().find(|bar| {
            bar.bar_index == 0
                && matches!(bar.space, PcieBarSpace::Memory32 | PcieBarSpace::Memory64)
        }) else {
            error!("xhci: {}:{}.{}, BAR0 is missing", bdf.0, bdf.1, bdf.2);
            continue;
        };

        let old_command = unsafe { cfg_read16(bdf.0, bdf.1, bdf.2, PCI_COMMAND) };
        let command = old_command | PCI_COMMAND_MEMORY | PCI_COMMAND_MASTER;
        pci_enable_device(bdf.0, bdf.1, bdf.2, command);
        info!(
            "xhci: claim QEMU controller {:02x}:{:02x}.{} PCI_COMMAND {:#x}->{:#x}",
            bdf.0, bdf.1, bdf.2, old_command, command
        );

        match new_qemu_xhci_host(bar.build_bar_space()) {
            Ok(host) => {
                register_xhci_host(host);
                #[cfg(feature = "riscv64-qemu")]
                debug_irq_state(QEMU_XHCI_IRQ);
            }
            Err(reason) => error!(
                "xhci: probe {:02x}:{:02x}.{} failed: {}",
                bdf.0, bdf.1, bdf.2, reason
            ),
        }
    }
}
