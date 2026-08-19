//! 内核主函数。
//! 完成 BSS 清零、架构初始化、日志/设备/文件系统初始化并启动第一个用户任务。
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

// module_inception: 本项目采用 `foo/mod.rs` 负责导出、`foo/foo.rs` 承载实现的分文件布局。
#![allow(clippy::module_inception)]
// multiple_crate_versions: 部分第三方依赖存在不可直接统一的传递依赖版本。
#![allow(clippy::multiple_crate_versions)]

#![no_std]
#![no_main]
#![feature(panic_internals, const_trait_impl)]
extern crate alloc;

/// 导出 paste crate 供宏使用。
pub use paste;

mod arch;
// console 的实现依赖 driver，保持驱动模块先注册。
mod driver;
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
use crate::driver::gpu::inital_gpu;
use crate::fs::vfs::*;
use crate::logger::*;
use crate::memory::*;
use crate::root::RootFs;
use crate::task::run_first_task;
use crate::time::*;
use log::*;
pub use sbi::*;

/// 清空 BSS 段。
pub fn clear_bss() {
    extern "C" {
        fn sbss();
        fn ebss();
    }
    (sbss as *const () as usize..ebss as *const () as usize)
        .for_each(|a| unsafe { (a as *mut u8).write_volatile(0) });
}

/// BlueStarOS 内核入口。
#[no_mangle]
pub fn blue_main() -> ! {
    kprintln!("Enter BlueStarOS main");
    kprintln!("Clean BSS");
    clear_bss();

    // 平台初始化必须位于 clear_bss 之后，其中包含依赖 BSS 的静态页表状态。
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

    panic!("unreachable after run_first_task");
}
