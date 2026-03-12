// RISC-V 架构相关实现
pub mod task;
pub mod memory;
pub mod panic;
pub mod trap;
pub mod sbi;
pub mod time;
// 重新导出，让外部可以通过 crate::arch::TaskContext 访问
pub use task::TaskContext;
pub use task::__switch;
pub use trap::TrapContext;
pub use sbi::*;

use riscv::register::{stvec, utvec::TrapMode};
use crate::config::TRAP_BOTTOM_ADDR;
use core::{arch::global_asm,  panicking::panic};
use crate::{config::*, task::TASK_MANAER, time::set_next_timeInterupt};
use log::{debug, error, };
use riscv::register::{scause::{self, Exception, Trap}, sie::Sie, sscratch, sstatus::{self, SPP, Sstatus}, stval};
use crate::syscall::*;//系统调用
use riscv::register::sie;
use riscv::register::scause::Interrupt;
use core::arch::asm;
use crate::memory::{MapSet, VirNumRange};
use log::warn;
use riscv::register::satp;
use crate::arch::memory::*;
use alloc::vec::Vec;
use crate::sync::UPSafeCell;
use lazy_static::lazy_static;
use crate::trap::recycle_pending_kstacks;
use crate::trap::pagefaultHandler::PageFaultHandler;
use crate::kprintln;

// 引入trap
global_asm!(include_str!("trap.asm"));

// 引入入口
global_asm!(include_str!("./entry.asm"));

/// 平台初始化函数
pub fn arch_init(){

}

/// 设置内核陷阱处理程序向量
pub fn set_kernel_trap_handler() {
    unsafe {
        let trap_entry = TRAP_BOTTOM_ADDR as usize;
        stvec::write(trap_entry, TrapMode::Direct);
    }
}

/// 设置内核禁止陷阱处理
pub fn set_kernel_forbid() {
    unsafe {
        stvec::write(kernel_traped_forbid as usize, TrapMode::Direct);
    }
}

///愿意处理全局中断。   这个状态会被trapcontext读取
pub fn rather_global_interrupt(){
        let sstatus_raw = sstatus::read();
    
    // 打印调试信息
    debug!("Initial sstatus value:");
    debug!("  SIE  (bit 1): {}", (sstatus_raw.bits() >> 1) & 1);
    debug!("  SPIE (bit 5): {}", (sstatus_raw.bits() >> 5) & 1);
    debug!("  SPP  (bit 8): {}", (sstatus_raw.bits() >> 8) & 1);
    unsafe {
        sstatus::set_spie();
    }
}


///设置sstatus的sie开启全局中断使能，设置sie寄存器的第五位（从0开始）开启具体时钟中断 关键雷区，在内核不开sie，仅仅设置stie，在第一个任务sret会恢复到sie上，从而开启中断
pub fn enable_timer_interupt(){
    unsafe {
     //sstatus::set_sie(); //先暂时不开内核全局中断使能   内核中断会错误
     sie::set_stimer(); 
    }
    debug!("TIMER INTERUPT ENABLE!");
}

///设置sstatus的外部中断使能
pub fn enable_external_interrupt(){
    unsafe {
        sie::set_sext();//全局中断使能未开启
    }
}

fn log_user_fault_detail(tag: &str) {
    let scauses = scause::read();
    let sepc_val = sepc::read();
    let stval_val = stval::read();
    let sstatus_val = sstatus::read();
    let satp_val = satp::read();
    error!(
        "{}: cause={:?} sepc={:#x} stval={:#x} va={:#x} sstatus={:#x} satp={:#x}",
        tag,
        scauses.cause(),
        sepc_val,
        stval_val,
        stval_val,
        sstatus_val.bits(),
        satp_val.bits()
    );
}

/// 第一次进入用户态的入口点
/// __switch 会跳转到这里，设置好 trap 环境后跳转到用户态
#[no_mangle]
pub extern "C" fn app_entry_point() {
    set_kernel_trap_handler();
    let user_satp = TASK_MANAER.get_current_stap();
    let restore_va = __kernel_refume as usize - __kernel_trap as usize + TRAP_BOTTOM_ADDR;
    //error!("Resrore_va:{:#x}",restore_va);
   // let restore_va = __kernel_refume as usize;
    // trace!("[kernel] trap_return: ..before return");
   //debug!("Welcome to app entry point!!! user_satp:{:#x}",user_satp);
    unsafe {
        asm!(
            "fence.i",
            "jr {restore_va}",         // jump to new addr of __restore asm function
            restore_va = in(reg) restore_va,
            in("a0") TRAP_CONTEXT_ADDR,      // a0 = virt addr of Trap Context
            in("a1") user_satp,        // a1 = phy addr of usr page table
            options(noreturn)
        );
    }
}



use riscv::register::sepc;
///handler必须返回到trap里面去
pub extern "C" fn kernel_trap_handler(){//内核专属trap（目前不应该被调用）
    
    // 回收内核栈
    recycle_pending_kstacks();
    
    set_kernel_forbid();
    let scauses = scause::read();
    let sepc_val = sepc::read();
    let stval_val = stval::read();
    let (sys_id, sys_args) = {
        let current_trapcx = TASK_MANAER.get_current_trapcx();
        let id = current_trapcx.x[17];
        let args = [
            current_trapcx.x[10],
            current_trapcx.x[11],
            current_trapcx.x[12],
            current_trapcx.x[13],
            current_trapcx.x[14],
            current_trapcx.x[15],
        ];
        (id, args)
    };
        match scauses.cause(){
        Trap::Exception(Exception::UserEnvCall)=>{
            {
                let current_trapcx = TASK_MANAER.get_current_trapcx();
                debug!("pre sepc:{:#x}",current_trapcx.sepc_entry_point);
                current_trapcx.sepc_entry_point += 4;
            }
            // 注意：sys_exec 可能会替换地址空间并重建 TrapContext，不能持有旧 trapcx 引用跨 syscall。
            let ret = syscall_handler(sys_id, sys_args);
            {
                let current_trapcx: &mut TrapContext = TASK_MANAER.get_current_trapcx();
                debug!("lat sepc:{:#x}",current_trapcx.sepc_entry_point);
                current_trapcx.x[10] = ret as usize;
            }
        }
        Trap::Exception(Exception::IllegalInstruction)=>{
            error!("User IllegalInstruction at {:#x}", sepc_val);
            TASK_MANAER.kail_current_task_and_run_next();
            //panic!("User IllegalInstruction at {:#x}", sepc_val)
        }
        Trap::Exception(Exception::InstructionPageFault)=>{
            log_user_fault_detail("User InstructionPageFault");
            PageFaultHandler(VirAddr(stval_val),scauses);
        }
        Trap::Exception(Exception::LoadPageFault)=>{
            log_user_fault_detail("User LoadPageFault");
            PageFaultHandler(VirAddr(stval_val),scauses);
        }
        Trap::Exception(Exception::StorePageFault)=>{
            log_user_fault_detail("User StorePageFault");
            PageFaultHandler(VirAddr(stval_val),scauses);
        }
        Trap::Interrupt(Interrupt::SupervisorTimer)=>{

            // 处理进程信号
            TASK_MANAER.resolve_current_task_signal();
           
            set_next_timeInterupt();

            TASK_MANAER.suspend_and_run_task();
        }
        Trap::Interrupt(Interrupt::SupervisorExternal)=>{
            //外部中断，键盘等
            panic!("externnal interrupt,but rust sbi make complete abtract!");
        }
        _=>{
            panic!("Unknown trap from user: {:?}", scauses.cause())
        }
    }
    app_entry_point();//传入特定参数，返回回去
}

pub fn no_return_start()->!{
    panic("Start Function you ret ,WTF????");
}



#[no_mangle]
pub extern "C" fn skernel_traped_forbid(){
    let scauses = scause::read();
    let sepc_val = sepc::read();
    let stval_val = stval::read();
    let sstatus_val = sstatus::read();
    let satp_val = satp::read();

    kprintln!(
        "UnSupport Kernel Trap: cause={:?} sepc={:#x} stval={:#x} sstatus={:#x} satp={:#x}",
        scauses.cause(),
        sepc_val,
        stval_val,
        sstatus_val.bits(),
        satp_val.bits()
    );
    shutdown();

}
