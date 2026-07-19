pub mod board;
pub mod driver;
pub mod memory;
pub mod panic;
pub mod sbi;
pub mod task;
pub mod time;
pub mod trap;

use crate::allocator_init;
use crate::arch::driver::{gicd, keyboard};
use crate::arch::memory::early_mmu_init;
use crate::arch::memory::eaylymmu::turn_early_mmu;
use crate::config::*;
use crate::debug;
use crate::dtb;
use crate::init_frame_allocator_from_dtb;
use crate::kernel_info_debug;
use crate::kprintln;
use crate::logger;
use core::arch::{asm, global_asm};

pub use sbi::*;
global_asm!(include_str!("./entry.asm"));

pub use task::__switch;
pub use trap::{
    __aarch64_vector, app_entry_point, kernel_trap_handler, set_kernel_trap_handler, TrapContext,
};

pub fn arch_init() {
    kprintln!("Arch PlatForm init");

    unsafe {
        early_mmu_init();
    }

    turn_early_mmu();
    allocator_init();
    dtb::init();

    kprintln!("Logger start inited");
    logger::init();
    kprintln!("Logger inited");
    kprintln!("Inital Physical Memory Alloctor");
    init_frame_allocator_from_dtb(ekernel as usize);
    dtb::run_device_probes();
    kernel_info_debug();

    kprintln!("Welcome to BlueStarOS!");
    debug!("Kernel init success!");

    set_kernel_trap_handler();
    KERNEL_SPACE.lock(|ks| ks.activate());
    disable_timer_interrupt();
    gicd::gic_init();
    keyboard::enable_uart_rx_interrupt();
    kprintln!("[ArchInit] keyboard irq ready");

    rather_global_interrupt();
    kprintln!("[ArchInit] global irq enabled");
    kprintln!("[ArchInit] timer irq disabled for qemu irq debug");
}

pub fn rather_global_interrupt() {
    enable_irq();
}

pub fn enable_irq() {
    unsafe {
        asm!("msr daifclr, #2");
    }
}

pub fn disable_irq() {
    unsafe {
        asm!("msr daifset, #2");
    }
}

pub fn wait_for_interrupt() {
    unsafe {
        asm!("wfi");
    }
}

pub fn enable_timer_interrupt() {
    unsafe {
        asm!("msr cntp_ctl_el0, {}", in(reg) 1u64);
    }
}

pub fn disable_timer_interrupt() {
    unsafe {
        asm!("msr cntp_ctl_el0, {}", in(reg) 0u64);
    }
}
