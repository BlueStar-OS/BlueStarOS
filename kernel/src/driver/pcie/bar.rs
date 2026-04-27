use crate::driver::pcie::*;
use crate::pcie_log;
static mut PCI_MMIO_ALLOC: u64 = 0x4000_0000;
const PCI_MMIO_END: u64 = 0x8000_0000;

#[derive(Clone, Copy)]
pub enum BarKind {
    Io,
    Mem32,
    Mem64,
}

pub struct PciBar {
    raw: u32,
    kind: BarKind,
    base: u64,
    prefetchable: bool,
}

pub fn decode_bar(raw: u32, next_raw: Option<u32>) -> Option<PciBar> {
    if raw == 0 {
        return None;
    }

    if raw & 0x1 != 0 {
        return Some(PciBar {
            raw,
            kind: BarKind::Io,
            base: (raw & 0xffff_fffc) as u64,
            prefetchable: false,
        });
    }

    let mem_type = (raw >> 1) & 0x3;
    let prefetchable = (raw & 0x8) != 0;

    match mem_type {
        0b00 => Some(PciBar {
            raw,
            kind: BarKind::Mem32,
            base: (raw & 0xffff_fff0) as u64,
            prefetchable,
        }),
        0b10 => {
            let hi = next_raw.unwrap_or(0) as u64;
            let lo = (raw & 0xffff_fff0) as u64;
            Some(PciBar {
                raw,
                kind: BarKind::Mem64,
                base: (hi << 32) | lo,
                prefetchable,
            })
        }
        _ => None,
    }
}

fn align_up(x: u64, align: u64) -> u64 {
    (x + align - 1) & !(align - 1)
}

pub fn alloc_pci_mmio(size: u64) -> u64 {
    unsafe {
        let base = align_up(PCI_MMIO_ALLOC, size);
        PCI_MMIO_ALLOC = base + size;
        assert!(PCI_MMIO_ALLOC <= PCI_MMIO_END);
        pcie_log!("bar alloc memory {:#x} bytes on addrress:{:#x}", size, base);
        base
    }
}

pub fn assign_bar0(bus: u8, dev: u8, func: u8) -> Option<(u64, u64)> {
    let raw = unsafe { cfg_read32(bus, dev, func, PCI_BAR0) };
    if raw & 1 != 0 {
        return None;
    }

    let is_64 = (raw & 0x6) == 0x4;
    let size = bar_size32(bus, dev, func, PCI_BAR0) as u64;
    if size == 0 {
        return None;
    }

    let base = alloc_pci_mmio(size);

    let low = (base as u32) & 0xffff_fff0;
    unsafe { cfg_write32(bus, dev, func, PCI_BAR0, low | (raw & 0xf)) };

    if is_64 {
        unsafe { cfg_write32(bus, dev, func, PCI_BAR0 + 4, (base >> 32) as u32) };
    }

    Some((base, size))
}

/// detect bar size. bits size
pub fn bar_size32(bus: u8, dev: u8, func: u8, bar_off: u16) -> u32 {
    //TODO: DISABLE COMMAND BEFORE SIZING

    let old = unsafe { cfg_read32(bus, dev, func, bar_off) };

    unsafe { cfg_write32(bus, dev, func, bar_off, 0xffff_ffff) };
    let mask = unsafe { cfg_read32(bus, dev, func, bar_off) };

    unsafe { cfg_write32(bus, dev, func, bar_off, old) };

    if mask == 0 || mask == 0xffff_ffff {
        return 0;
    }
    if mask & 1 != 0 {
        let m = mask & 0xffff_fffc;
        (!m).wrapping_add(1)
    } else {
        let m = mask & 0xffff_fff0;
        (!m).wrapping_add(1)
    }
}
