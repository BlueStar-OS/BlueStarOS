//!内核主函数
//! bss段初始化，日志系统初始化，物理页帧分配系统初始化，内核地址空间初始化，激活内核地址空间
//#! 
//#![deny(missing_docs)]
//#![deny(warnings)]
#![no_std]
#![no_main]
#![feature(panic_internals,panic_info_message,const_trait_impl,error_in_core)]

extern crate alloc;

// 导出 paste crate 供宏使用
pub use paste;


mod arch;  // 架构抽象层
mod driver;  // driver 必须在 console 之前加载，因为 console 依赖 driver
#[macro_use]
mod console;
mod panic;
mod config;
mod error;
mod logger;
mod memory;
mod sync;
mod syscall;
mod trap;
mod time;
mod task;
mod fs;
use crate::arch::*;
use crate::driver::dtb;
use crate::sync::UPSafeCell;
use log::*;
use crate::fs::vfs::*;
use crate::task::run_first_task;
use crate::time::*;
use crate::memory::*;
use crate::logger::*;
use crate::config::*;
use crate::arch::set_kernel_trap_handler;
use core::arch::global_asm;
use core::sync::atomic::AtomicU32;
use core::sync::atomic::Ordering;
pub use sbi::*;

global_asm!(include_str!("app.asm"));

/// clear BSS segment
pub fn clear_bss() {
    extern "C" {
        pub fn sbss();
        pub fn ebss();
    }
    (sbss as usize..ebss as usize).for_each(|a| unsafe { (a as *mut u8).write_volatile(0) });
}

pub fn kernel_init(){
    kprintln!("Clean BSS");
    clear_bss();//清空bss

    kprintln!("Arch PlatForm init");

    
    
   

    //平台初始化 必须放在clearbss之后！ 因为其中有bss的静态初始化页表相关
    arch_init();


     //内核堆，分配器初始化
    allocator_init();

    // 初始化设备树,运行设备探针
    dtb::init();

    

    kprintln!("Logger start inited");
    logger::init();//日志初始化 - 必须先初始化日志才能使用 debug!
    kprintln!("Logger inited");
    kprintln!("Inital Physical Memory Alloctor");
    init_frame_allocator_from_dtb(ekernel as usize);//物理内存页分配器初始化（从DTB探测）
    kernel_info_debug();//打印内核日志
    
}

/// the rust entry-point of os
#[no_mangle]
pub fn blue_main() -> ! {//永远不会返回

    
    
    kprintln!("Welcome to BlueStarOS!");
    kernel_init(); //bss，日志，分配器初始化

    debug!("Kernel init success!");

    set_kernel_trap_handler();//初始化陷阱入口，必须在地址空间激活后设置虚拟地址


    KERNEL_SPACE.lock().activate();//激活地址空间


    // 下面需要在内核空间开启之后执行，因为涉及内核态中断访问trap
    rather_global_interrupt();//愿意处理全局中断使能
    enable_timer_interupt();//开启全局时间中断使能
    set_next_timeInterupt();//第一次开启时钟中断


    
    debug!("stext {:#x}",__kernel_trap as usize);

    debug!("traper {:#x}",straper as usize);
    debug!("trap refume virtualaddr:{:#x}",__kernel_refume as usize - __kernel_trap as usize + TRAP_BOTTOM_ADDR);
   
    warn!("initial file system");
    RootFs::init_rootfs();
    warn!("initial gpu driver");
    // use crate::driver::init_gpu;
    // init_gpu(); //初始化virtio gpu设备
    
    run_first_task();

    warn!("All right,kernel Will end\n");
    panic!("Kernel End");

    

}
