pub mod driver;
pub mod ipi;
pub mod memory;
pub mod panic;
pub mod sbi;
pub mod task;
pub mod time;
pub mod trap;

use crate::allocator_init;
use crate::config::*;
use crate::dtb;
use crate::init_frame_allocator_from_dtb;
use crate::kernel_info_debug;
use crate::kprintln;
use crate::logger;
use crate::set_next_time_interupt;
pub use core::arch::asm;
use core::arch::global_asm;
use core::sync::atomic::fence;
use core::sync::atomic::Ordering::Release;
use log::debug;
pub use riscv::register::satp;
use riscv::register::sie;
use riscv::register::sstatus;
use riscv::register::stvec;
use riscv::register::utvec::TrapMode;

pub use trap::{kernel_trap_handler, TrapContext};

extern "C" {
    fn __kernel_mode_trap();
}

global_asm!(include_str!("trap.asm"));
global_asm!(include_str!("kernel_trap.asm"));
global_asm!(include_str!("./entry.asm"));

pub fn arch_init() {
    kprintln!("Arch PlatForm init");

    allocator_init();
    dtb::init();

    fence(Release);

    kprintln!("Logger start inited");
    logger::init();
    kprintln!("Logger inited");
    kprintln!("Inital Physical Memory Alloctor");

    init_frame_allocator_from_dtb(ekernel as *const () as usize);
    driver::init_external_interrupts();

    kprintln!("Welcome to BlueStarOS!");
    debug!("Kernel init success!");

    // 目前在内核就先设置内核trap,在进入用户态前再切换回usertrap
    set_kernel_trap();
    // 开启内核中断
    enable_irq();

    // 设备 probe 可能启动控制器并产生中断，因此必须在 trap 和 IRQ 就绪后执行。
    // KERNEL_SPACE 仍然在 probe 后激活，以便收集 probe 注册的全部 MMIO 区域。
    dtb::run_device_probes();
    kernel_info_debug();

    KERNEL_SPACE.lock(|ks| ks.activate());

    rather_global_interrupt();
    enable_timer_interrupt();
    set_next_time_interupt();
}

pub fn set_kernel_trap_handler() {
    unsafe {
        let trap_entry = TRAP_BOTTOM_ADDR;
        stvec::write(trap_entry, TrapMode::Direct);
    }
}

pub fn set_kernel_trap() {
    unsafe {
        stvec::write(__kernel_mode_trap as *const () as usize, TrapMode::Direct);
    }
}

pub fn rather_global_interrupt() {
    let sstatus_raw = sstatus::read();

    debug!("Initial sstatus value:");
    debug!("  SIE  (bit 1): {}", (sstatus_raw.bits() >> 1) & 1);
    debug!("  SPIE (bit 5): {}", (sstatus_raw.bits() >> 5) & 1);
    debug!("  SPP  (bit 8): {}", (sstatus_raw.bits() >> 8) & 1);
    unsafe {
        sstatus::set_spie();
    }
}

pub fn enable_irq() {
    unsafe {
        sstatus::set_sie();
    }
}

pub fn disable_irq() {
    unsafe {
        sstatus::clear_sie();
    }
}

pub fn wait_for_interrupt() {
    unsafe {
        asm!("wfi");
    }
}

pub fn enable_timer_interrupt() {
    unsafe {
        sie::set_stimer();
    }
}

pub fn disable_timer_interrupt() {
    unsafe {
        sie::clear_stimer();
    }
}

pub fn enable_external_interrupt() {
    unsafe {
        sie::set_sext();
    }
}
