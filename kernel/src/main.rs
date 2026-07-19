//!内核主函数
//! bss段初始化，日志系统初始化，物理页帧分配系统初始化，内核地址空间初始化，激活内核地址空间
// 在 main.rs 或 lib.rs 顶部添加（根据你的工程结构）
#![deny(clippy::doc_markdown)]
#![deny(clippy::single_match_else)]
#![deny(clippy::struct_field_names)]
#![deny(clippy::should_implement_try_from)]
#![deny(clippy::unnecessary_semicolon)]
#![deny(clippy::semicolon_if_nothing_returned)]
#![deny(clippy::match_like_matches_macro)]
#![deny(clippy::match_same_arms)]
#![deny(clippy::redundant_continue)]
#![deny(clippy::unnecessary_wraps)]
#![deny(clippy::missing_errors_doc)]
#![deny(clippy::missing_panics_doc)]
#![deny(non_snake_case)]
#![deny(unused_must_use)]
#![deny(non_upper_case_globals)]
#![deny(static_mut_refs)]
#![deny(clippy::correctness)]
#![deny(clippy::suspicious)]
#![deny(clippy::complexity)]
#![deny(clippy::perf)]
#![deny(clippy::cargo)]
#![deny(clippy::all)]
#![deny(missing_docs)]
#![deny(clippy::empty_line_after_doc_comments)]
#![deny(clippy::missing_safety_doc)]
#![deny(clippy::style)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]

// —— 有意豁免的 lint（附理由）——
// module_inception: 本项目遵循 CLAUDE.md 的“代码文件模块化”规范，采用
//   `foo/mod.rs` 只做导出、`foo/foo.rs` 承载实现的分文件布局，必然触发
//   `mod foo { ... mod foo }` 结构，属架构预期，非缺陷。
#![allow(clippy::module_inception)]
// multiple_crate_versions: spin(0.7 经 buddy_system_allocator) 与
//   bitflags(1.x 经 riscv/virtio-drivers) 的多版本来自第三方传递依赖，
//   非本 crate 直接可控，不 fork 上游无法统一。
#![allow(clippy::multiple_crate_versions)]

// 以下非 lint 属性原样保留
#![no_std]
#![no_main]
#![feature(panic_internals, const_trait_impl)]
extern crate alloc;

// 导出 paste crate 供宏使用
pub use paste;

mod arch; // 架构抽象层
mod driver; // driver 必须在 console 之前加载，因为 console 依赖 driver
#[macro_use]
mod console;
mod config;
mod error;
mod fs;
mod logger;
mod memory;
pub mod network;
mod panic;
mod symbols;
mod sync;
mod syscall;
mod task;
mod time;
mod tool;
mod trap;
use crate::arch::*;
use crate::config::*;
use crate::driver::dtb;
use crate::driver::gpu::inital_gpu;
use crate::fs::vfs::*;
use crate::logger::*;
use crate::memory::*;
use crate::root::RootFs;
use crate::task::run_first_task;
use crate::time::*;
use core::arch::global_asm;
use core::ptr::read_volatile;
use core::ptr::write_volatile;
use log::*;
pub use sbi::*;

global_asm!(include_str!("app.asm"));

/// clear BSS segment
pub fn clear_bss() {
    extern "C" {
        pub fn sbss();
        pub fn ebss();
    }
    (sbss as *const () as usize..ebss as *const () as usize)
        .for_each(|a| unsafe { (a as *mut u8).write_volatile(0) });
}

/// the rust entry-point of os
#[no_mangle]
pub fn blue_main() -> ! {
    kprintln!("Enter BlueStarOS main");
    //永远不会返回
    kprintln!("Clean BSS");
    clear_bss();

    //平台初始化 必须放在clearbss之后！ 因为其中有bss的静态初始化页表相关
    arch_init();

    debug!("stext {:#x}", __kernel_trap as *const () as usize);

    debug!("traper {:#x}", straper as *const () as usize);
    debug!(
        "trap refume virtualaddr:{:#x}",
        __kernel_refume as *const () as usize - __kernel_trap as *const () as usize
            + TRAP_BOTTOM_ADDR
    );
    inital_gpu();
    warn!("initial file system");

    RootFs::init_rootfs();

    run_first_task();

    warn!("All right,kernel Will end\n");
    panic!("Kernel End");
}
