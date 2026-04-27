use crate::driver::pcie::PhysiAddr;
use crate::driver::pcie::PCIE_ECAM_ADDR;
use log::error;
// shift
pub const bus_shift: u32 = 20;
pub const dev_shift: u32 = 15;
pub const func_shift: u32 = 12;

pub const device_per_bus: u8 = 32;
pub const function_per_device: u8 = 8;

pub const funcion_memory_size: u32 = 4096;

/// get ecam function addr
/// return physical addr
fn get_ecam_addr_of_bus(bus_no: u8, dev_no: u8, func_no: u8, offset: u16) -> PhysiAddr {
    let eacm_base_add = unsafe { PCIE_ECAM_ADDR };
    if eacm_base_add == 0 {
        error!("[PCIE]:eacm_base_add is zero!");
        return PhysiAddr(0);
    }
    let addr: usize = eacm_base_add
        + (((bus_no as usize) << bus_shift)
            + ((dev_no as usize) << dev_shift)
            + ((func_no as usize) << func_shift)) as usize
        + offset as usize;
    return PhysiAddr(addr);
}

pub unsafe fn cfg_read32(bus: u8, dev: u8, func: u8, offset: u16) -> u32 {
    let addr = get_ecam_addr_of_bus(bus, dev, func, offset);
    core::ptr::read_volatile(addr.0 as *const u32)
}

pub unsafe fn cfg_read16(bus: u8, dev: u8, func: u8, offset: u16) -> u16 {
    let aligned = cfg_read32(bus, dev, func, offset & !3);
    let shift = ((offset & 3) * 8) as u32;
    ((aligned >> shift) & 0xffff) as u16
}

pub unsafe fn cfg_write32(bus: u8, dev: u8, func: u8, offset: u16, val: u32) {
    let addr = get_ecam_addr_of_bus(bus, dev, func, offset);
    core::ptr::write_volatile(addr.0 as *mut u32, val);
}

pub unsafe fn cfg_write16(bus: u8, dev: u8, func: u8, offset: u16, val: u16) {
    let addr = get_ecam_addr_of_bus(bus, dev, func, offset);
    core::ptr::write_volatile(addr.0 as *mut u16, val);
}
