use core::arch::asm;
use log::{debug, error, warn};


/// ESR_EL1 异常类别（EC）定义
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExceptionClass {
    Unknown = 0x00,
    SVC64 = 0x15,              // SVC指令（64位）
    InstructionAbortLower = 0x20,  // 指令异常（低EL）
    InstructionAbortSame = 0x21,   // 指令异常（当前EL）
    PCAlignment = 0x22,        // PC对齐错误
    DataAbortLower = 0x24,     // 数据异常（低EL）
    DataAbortSame = 0x25,      // 数据异常（当前EL）
    SPAlignment = 0x26,        // SP对齐错误
    BRK = 0x3C,                // BRK指令
}

impl ExceptionClass {
    pub fn from_esr(esr: u64) -> Self {
        let ec = ((esr >> 26) & 0x3F) as u8;
        match ec {
            0x15 => Self::SVC64,
            0x20 => Self::InstructionAbortLower,
            0x21 => Self::InstructionAbortSame,
            0x22 => Self::PCAlignment,
            0x24 => Self::DataAbortLower,
            0x25 => Self::DataAbortSame,
            0x26 => Self::SPAlignment,
            0x3C => Self::BRK,
            _ => Self::Unknown,
        }
    }
}

/// 读取ESR_EL1寄存器
#[inline]
fn read_esr_el1() -> u64 {
    let esr: u64;
    unsafe {
        asm!("mrs {}, esr_el1", out(reg) esr);
    }
    esr
}

/// 读取ELR_EL1寄存器（异常返回地址）
#[inline]
fn read_elr_el1() -> u64 {
    let elr: u64;
    unsafe {
        asm!("mrs {}, elr_el1", out(reg) elr);
    }
    elr
}

/// 读取FAR_EL1寄存器（错误地址）
#[inline]
fn read_far_el1() -> u64 {
    let far: u64;
    unsafe {
        asm!("mrs {}, far_el1", out(reg) far);
    }
    far
}

/// 读取SPSR_EL1寄存器（保存的程序状态）
#[inline]
fn read_spsr_el1() -> u64 {
    let spsr: u64;
    unsafe {
        asm!("mrs {}, spsr_el1", out(reg) spsr);
    }
    spsr
}

/// 打印异常详细信息
fn log_exception_detail(tag: &str) {
    let esr = read_esr_el1();
    let elr = read_elr_el1();
    let far = read_far_el1();
    let spsr = read_spsr_el1();
    let ec = ExceptionClass::from_esr(esr);

    error!(
        "{}: EC={:?} ESR={:#x} ELR={:#x} FAR={:#x} SPSR={:#x}",
        tag, ec, esr, elr, far, spsr
    );
}

// ============ 用户态异常处理（EL0 → EL1）============

/// 用户态同步异常处理
#[no_mangle]
pub extern "C" fn sync_el0_64() {
    log_exception_detail("Sync exception from EL0");
    panic!("Sync exception from EL0");
}

/// 处理系统调用
fn handle_syscall(syscall_num: usize) {
   
}

/// 处理页错误
fn handle_page_fault() {
    let far = read_far_el1();
    let esr = read_esr_el1();

    // TODO: 调用页错误处理器
    // PageFaultHandler(VirAddr(far as usize), esr);

    warn!("Page fault at {:#x}, killing task", far);
}

/// 用户态IRQ中断处理
#[no_mangle]
pub extern "C" fn irq_el0_64() {
    panic!("IRQ from EL0");
}

/// 用户态FIQ中断处理
#[no_mangle]
pub extern "C" fn fiq_el0_64() {
    panic!("FIQ from EL0");
}

/// 用户态SError处理
#[no_mangle]
pub extern "C" fn serror_el0_64() {
    log_exception_detail("SError from EL0");
    panic!("SError from EL0");
}

// ============ 内核态异常处理（EL1 → EL1）============

/// 内核态同步异常处理（使用SP_EL1）
#[no_mangle]
pub extern "C" fn sync_el1_spx() {
    log_exception_detail("Kernel sync exception (SP_EL1)");
    panic!("Kernel exception - this should not happen!");
}

/// 内核态IRQ中断处理（使用SP_EL1）
#[no_mangle]
pub extern "C" fn irq_el1_spx() {
    panic!("IRQ in kernel (SP_EL1)");
}

/// 内核态FIQ中断处理（使用SP_EL1）
#[no_mangle]
pub extern "C" fn fiq_el1_spx() {
    panic!("FIQ in kernel (SP_EL1)");
}

/// 内核态SError处理（使用SP_EL1）
#[no_mangle]
pub extern "C" fn serror_el1_spx() {
    log_exception_detail("SError in kernel (SP_EL1)");
    panic!("Kernel SError!");
}

/// 内核态同步异常处理（使用SP_EL0）
#[no_mangle]
pub extern "C" fn sync_el1_sp0() {
    log_exception_detail("Kernel sync exception (SP_EL0)");
    panic!("Kernel exception with SP_EL0!");
}

/// 内核态IRQ中断处理（使用SP_EL0）
#[no_mangle]
pub extern "C" fn irq_el1_sp0() {
    panic!("IRQ in kernel (SP_EL0)");
}

/// 内核态FIQ中断处理（使用SP_EL0）
#[no_mangle]
pub extern "C" fn fiq_el1_sp0() {
    panic!("FIQ in kernel (SP_EL0)");
}

/// 内核态SError处理（使用SP_EL0）
#[no_mangle]
pub extern "C" fn serror_el1_sp0() {
    log_exception_detail("SError in kernel (SP_EL0)");
    panic!("Kernel SError with SP_EL0!");
}

// ============ 32位用户态异常处理 ============

/// 32位用户态同步异常处理
#[no_mangle]
pub extern "C" fn sync_el0_32() {
    panic!("32-bit user mode not supported");
}

/// 32位用户态IRQ中断处理
#[no_mangle]
pub extern "C" fn irq_el0_32() {
    panic!("IRQ from 32-bit EL0");
}

/// 32位用户态FIQ中断处理
#[no_mangle]
pub extern "C" fn fiq_el0_32() {
    panic!("FIQ from 32-bit EL0");
}

/// 32位用户态SError处理
#[no_mangle]
pub extern "C" fn serror_el0_32() {
    panic!("SError from 32-bit EL0");
}
