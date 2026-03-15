use core::arch::asm;
use log::{debug, error, warn};
use crate::task::TASK_MANAER;
use crate::syscall::syscall_handler;
use crate::arch::memory::VirAddr;
use crate::time::set_next_timeInterupt;
use crate::arch::TrapContext;

// AArch64页错误处理适配器
fn handle_page_fault_aarch64(fault_addr: VirAddr, esr: u64) {
    use crate::arch::memory::{VirNumber, PageTable, PTEFlags};

    debug!("Handle Fault Virtual Address:{:#x}", fault_addr.0);
    let contain_vpn: VirNumber = fault_addr.floor_down();
    let tsak_satp = TASK_MANAER.get_current_stap();
    let mut map_layer: PageTable = PageTable::crate_table_from_satp(tsak_satp);

    // 解析ESR_EL1获取错误类型
    let ec = (esr >> 26) & 0x3F;
    let iss = esr & 0x1FFFFFF;
    let is_write = (iss & (1 << 6)) != 0;  // WnR bit
    let is_instruction = ec == 0x20 || ec == 0x21;

    // 检查页表项
    match &mut map_layer.find_pte_vpn(contain_vpn) {
        Some(pte) => {
            if pte.is_valid() {
                // 处理A/D位更新
                if pte.is_valid() && pte.flags().contains(PTEFlags::W) && is_write {
                    (*pte).set_isaccess();
                    (*pte).set_isdirty();
                    unsafe {
                        asm!("dsb sy", "tlbi vmalle1", "dsb sy", "isb");
                    }
                    warn!("Update pte access and dirty flags");
                    return;
                } else if pte.is_valid() && pte.flags().contains(PTEFlags::R) && !is_write {
                    (*pte).set_isaccess();
                    unsafe {
                        asm!("dsb sy", "tlbi vmalle1", "dsb sy", "isb");
                    }
                    warn!("Update pte access flags");
                    return;
                } else if !pte.is_valid() {
                    // 继续处理缺页
                } else {
                    error!("PageFault Unhandled! Killed.");
                    error!("  Addr: {:#x}", fault_addr.0);
                    error!("  ESR: {:#x}", esr);
                    error!("  PTE Flags: {:?}", pte.flags());
                    TASK_MANAER.kail_current_task_and_run_next();
                    return;
                }
            }
        }
        None => {
            // 路不通，继续处理
        }
    }

    // 检查是否有对应的mmap区域
    let inner = TASK_MANAER.task_que_inner.lock();
    let current = inner.current;
    drop(inner);
    let inner = TASK_MANAER.task_que_inner.lock();

    let will_kill: bool = {
        let memset = &mut inner.task_queen[current].lock().memory_set;
        !memset.is_mmap_vpn(contain_vpn)
    };

    if will_kill {
        error!("area not contain mmap addr kill!");
        drop(inner);
        TASK_MANAER.kail_current_task_and_run_next();
        return;
    }

    debug!("[PageFaultHandler]:ligel!");

    {
        let memset = &mut inner.task_queen[current].lock().memory_set;
        memset.findarea_allocFrame_and_setPte(contain_vpn);
    }

    drop(inner);
}

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

/// 用户态同步异常处理 - 主要的trap处理入口
#[no_mangle]
pub extern "C" fn sync_el0_64() {
    use crate::trap::recycle_pending_kstacks;
    use crate::arch::set_kernel_forbid;


    error!("User trap!");

    // 回收内核栈
    recycle_pending_kstacks();

    set_kernel_forbid();

    let esr = read_esr_el1();
    let elr = read_elr_el1();
    let far = read_far_el1();
    let ec = ExceptionClass::from_esr(esr);

    // 获取系统调用参数（如果是系统调用）
    let (sys_id, sys_args) = {
        let current_trapcx = TASK_MANAER.get_current_trapcx();
        let id = current_trapcx.x[8] as usize;  // AArch64: x8 是系统调用号
        let args = [
            current_trapcx.x[0] as usize,  // x0-x5 是参数
            current_trapcx.x[1] as usize,
            current_trapcx.x[2] as usize,
            current_trapcx.x[3] as usize,
            current_trapcx.x[4] as usize,
            current_trapcx.x[5] as usize,
        ];
        (id, args)
    };

    match ec {
        ExceptionClass::SVC64 => {
            // 系统调用
            {
                let current_trapcx = TASK_MANAER.get_current_trapcx();
                debug!("pre elr:{:#x}", current_trapcx.elr_el1);
                current_trapcx.elr_el1 += 4;  // SVC指令是4字节
            }

            let ret = syscall_handler(sys_id, sys_args);

            {
                let current_trapcx: &mut TrapContext = TASK_MANAER.get_current_trapcx();
                debug!("lat elr:{:#x}", current_trapcx.elr_el1);
                current_trapcx.x[0] = ret as usize;  // x0 是返回值
            }
        }
        ExceptionClass::InstructionAbortLower => {
            // 指令页错误
            error!("User InstructionAbort at {:#x}", far);
            handle_page_fault_aarch64(VirAddr(far as usize), esr);
        }
        ExceptionClass::DataAbortLower => {
            // 数据页错误
            log_exception_detail("User DataAbort");
            handle_page_fault_aarch64(VirAddr(far as usize), esr);
        }
        ExceptionClass::PCAlignment => {
            error!("User PC alignment fault at {:#x}", elr);
            TASK_MANAER.kail_current_task_and_run_next();
        }
        ExceptionClass::SPAlignment => {
            error!("User SP alignment fault at {:#x}", far);
            TASK_MANAER.kail_current_task_and_run_next();
        }
        ExceptionClass::BRK => {
            error!("User BRK instruction at {:#x}", elr);
            TASK_MANAER.kail_current_task_and_run_next();
        }
        _ => {
            log_exception_detail("Unknown user exception");
            panic!("Unknown trap from user: {:?}", ec);
        }
    }

    // 返回用户态
    use crate::arch::app_entry_point;
    app_entry_point();
}

/// GIC 中断分发 — claim → dispatch → EOI
fn gic_handle_irq() {
    use crate::arch::driver::gicd::{gic_read_iar, gic_write_eoir, TIMER_PPI_INTID, UART2_INTID};

    loop {
        let irqnr = gic_read_iar();
        // 1020-1023 = spurious，没有更多 pending 中断
        if irqnr >= 1020 {
            break;
        }
        match irqnr {
            TIMER_PPI_INTID => {
                // EL1 Physical Timer PPI
                set_next_timeInterupt();
            }
            UART2_INTID => {
                // UART2 SPI — 键盘输入
                crate::arch::driver::keyboard::keyboard_interrupt_handler();
            }
            _ => {
                warn!("未知中断 INTID={}", irqnr);
            }
        }
        gic_write_eoir(irqnr);
    }
}

/// 用户态IRQ中断处理
#[no_mangle]
pub extern "C" fn irq_el0_64() {
    use crate::trap::recycle_pending_kstacks;

    // 回收内核栈
    recycle_pending_kstacks();

    // GIC claim → dispatch → EOI
    gic_handle_irq();

    // 处理进程信号
    TASK_MANAER.resolve_current_task_signal();

    // 挂起当前任务并运行下一个
    TASK_MANAER.suspend_and_run_task();

    // 返回用户态
    use crate::arch::app_entry_point;
    app_entry_point();
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
/// 由 __kernel_irq_entry 汇编入口保存/恢复寄存器后调用
#[no_mangle]
pub extern "C" fn irq_el1_spx() {
    // 内核态只处理中断，不做任务调度
    gic_handle_irq();
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
