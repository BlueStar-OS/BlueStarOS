//! Global logger

use log::{Level, LevelFilter, Log, Metadata, Record, debug};
use crate::{config::*, time::get_time_ms};

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
            Level::Warn => 93,  // BrightYellow
            Level::Info => 34,  // Blue
            Level::Debug => 32, // Green
            Level::Trace => 90, // BrightBlack
        };
        crate::kprintln!(
            "\u{1B}[{}m[{}][{:>5}] {}\u{1B}[0m",
            color,
            get_time_ms(),      
            record.level(),
            record.args(),
        );
    }
    fn flush(&self) {}
}
/// initiate logger
pub fn init() {
    static LOGGER: SimpleLogger = SimpleLogger;
    log::set_logger(&LOGGER).unwrap();
    log::set_max_level(match option_env!("LOG") {
        Some("ERROR") => LevelFilter::Error,
        Some("WARN") => LevelFilter::Warn,
        Some("INFO") => LevelFilter::Info,
        Some("DEBUG") => LevelFilter::Debug,
        Some("TRACE") => LevelFilter::Trace,
        _ => LevelFilter::Off,
    });
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
pub fn kernel_info_debug(){
    let skernle:usize=skernel as usize;
    let ekernle:usize=ekernel as usize;
    let stext:usize=stext as usize;
    let etext:usize=etext as usize;
    let srodata:usize=srodata as usize;
    let erodata:usize=erodata as usize;
    let sdata:usize=sdata as usize;
    let edata:usize=edata as usize;
    let sbss:usize=sbss as usize;
    let ebss:usize=ebss as usize;
    debug!("Kernel start at {:#x} ,End at: {:#x}",skernle,ekernle);
    debug!(".text start at {:#x} ,End at: {:#x}",stext,etext);
    debug!(".rodata start at {:#x} ,End at: {:#x}",srodata,erodata);
    debug!(".data start at {:#x} ,End at: {:#x}",sdata,edata);
    debug!(".bss start at {:#x} ,End at: {:#x}",sbss,ebss);
    debug!(".kernelStack start at {:#x} ,End at: {:#x}",kernel_stack_lower_bound as usize,kernel_stack_top as usize);
    debug!(".kernelStack start at {:#x} ,End at: {:#x}",kernel_trap_stack_bottom as usize,kernel_trap_stack_top as usize);
    
    
}