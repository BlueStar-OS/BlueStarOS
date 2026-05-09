//! Global logger

use crate::{config::*, time::get_time_us};
use log::{Level, LevelFilter, Log, Metadata, Record};

/// a simple logger
struct SimpleLogger;

impl Log for SimpleLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }
    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let color = match record.level() {
            Level::Error => 31, // Red
            Level::Warn => 35,  // BrightYellow
            Level::Info => 34,  // Blue
            Level::Debug => 32, // Green
            Level::Trace => 90, // BrightBlack
        };
        let us = get_time_us();
        let sec = us / 1_000_000;
        let usec = us % 1_000_000;
        // 分两步打印：前缀（时间辍 + level）再拼接 record.args()
        let module = record.module_path().unwrap_or("");
        crate::console::kprint(format_args!(
            "\u{1B}[{}m[{:>5}.{:06}] {:>5} {}: ",
            color,
            sec,
            usec,
            record.level(),
            module,
        ));
        crate::console::kprint(*record.args());
        crate::console::kprint(format_args!("\u{1B}[0m\n"));
    }
    fn flush(&self) {}
}
/// initiate logger
pub fn init() {
    static LOGGER: SimpleLogger = SimpleLogger;
    let err = log::set_logger(&LOGGER).err();
    if err.is_some() {
        kprintln!("Error Occuput :{} ", err.unwrap());
    }
    log::set_max_level(match option_env!("LOG") {
        Some("ERROR") => LevelFilter::Error,
        Some("WARN") => LevelFilter::Warn,
        Some("INFO") => LevelFilter::Info,
        Some("DEBUG") => LevelFilter::Debug,
        Some("TRACE") => LevelFilter::Trace,
        _ => LevelFilter::Off,
    });
    kprintln!("Start set logger end");
}

/*
pub fn kernel_stack_lower_bound();
pub fn kernel_stack_top();
pub fn ekernel();
pub fn skernel();
pub fn stext();
pub fn etext();
pub fn srodata();
pub fn erodata();
pub fn sdata();
pub fn edata();
pub fn sbss();
pub fn ebss(); */
pub fn kernel_info_debug() {
    use log::warn;
    let skernle: usize = skernel as *const () as usize;
    let ekernle: usize = ekernel as *const () as usize;
    let stext: usize = stext as *const () as usize;
    let etext: usize = etext as *const () as usize;
    let srodata: usize = srodata as *const () as usize;
    let erodata: usize = erodata as *const () as usize;
    let sdata: usize = sdata as *const () as usize;
    let edata: usize = edata as *const () as usize;
    let sbss: usize = sbss as *const () as usize;
    let ebss: usize = ebss as *const () as usize;
    warn!("Kernel start at {:#x} ,End at: {:#x}", skernle, ekernle);
    warn!(".text start at {:#x} ,End at: {:#x}", stext, etext);
    warn!(".rodata start at {:#x} ,End at: {:#x}", srodata, erodata);
    warn!(".data start at {:#x} ,End at: {:#x}", sdata, edata);
    warn!(".bss start at {:#x} ,End at: {:#x}", sbss, ebss);
    warn!(
        ".kernelStack start at {:#x} ,End at: {:#x}",
        kernel_stack_lower_bound as *const () as usize, kernel_stack_top as *const () as usize
    );

    warn!(
        "kernel_stack_protect start at {:#x} ,End at: {:#x}",
        kernel_stack_protect_start as *const () as usize,
        kernel_stack_protect_end as *const () as usize
    );

    warn!(
        "Kernel stack at {:#x} ,End at: {:#x}",
        kernel_stack_lower_bound as *const () as usize, kernel_stack_top as *const () as usize
    );

    warn!(
        "kernel_trap_stack_protect start at {:#x} ,End at: {:#x}",
        kernel_trap_stack_protect_start as *const () as usize,
        kernel_trap_stack_protect_end as *const () as usize
    );

    warn!(
        "kernel_trap_stack bottom at {:#x} ,Top at: {:#x}",
        kernel_trap_stack_bottom as *const () as usize, kernel_trap_stack_top as *const () as usize
    );
}
