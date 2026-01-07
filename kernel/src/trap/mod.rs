
use core::{arch::global_asm, panic, panicking::panic};
use crate::{config::*, task::TASK_MANAER, time::set_next_timeInterupt, trap::pagefaultHandler::PageFaultHandler};
use log::{debug, error, };
use riscv::register::{scause::{self, Exception, Trap}, sie::Sie, sscratch, sstatus::{self, SPP, Sstatus}, stval, stvec, utvec::TrapMode};
use crate::syscall::*;//系统调用
use riscv::register::sie;
use riscv::register::scause::Interrupt;
use core::arch::asm;
use crate::memory::VirAddr;
use log::warn;
use riscv::register::satp;

mod pagefaultHandler;

pub enum TrapFunction{
    USERHANDLER,
    KERNELHANDLER
}

#[repr(C)]
#[repr(align(8))]  // 确保 8 字节对齐
pub struct TrapContext{
     ///32个寄存器完全保存
     pub x:[usize;32],
     ///陷入状态
     pub sstatus:Sstatus, //32*8(sp)
     ///返回地址
     pub sepc_entry_point:usize,//33*8(sp)
     ///内核地址空间satp
     pub kernel_satp:usize,//34*8(sp)
     ///内核栈指针
     pub kernel_sp:usize,//35*8(sp)
     ///陷阱处理程序
     pub trap_handler:usize,//36*8(sp)
}



impl TrapContext {
    /// 初始化应用的 TrapContext,设置usersp
    pub fn init_app_trap_context(
        entry: usize,
        kernel_satp: usize,
        trap_handler: usize,
        kernel_sp: usize,
        user_sp:usize,
    ) -> Self {
        let mut sstatus = sstatus::read();
        sstatus.set_spp(SPP::User);  // 设置返回用户态
        let mut register = [0; 32];
        //x2
        debug!("SSTATUS:{:#X}",sstatus.bits());
        register[2]=user_sp;
        register[1]=no_return_start as usize;
        TrapContext {
            x: register,               // 通用寄存器初始化为 0，x[2](sp) 会在外部设置
            sstatus,
            sepc_entry_point: entry,   // 用户程序入口
            kernel_satp,              // 内核页表
            kernel_sp,                // 内核栈指针
            trap_handler,             // trap 处理函数
        }
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



pub fn set_kernel_trap_handler(){
    unsafe {
        let hander_func=kernel_trap_handler as usize;
        let trap_entry = TRAP_BOTTOM_ADDR as usize;
        stvec::write(trap_entry, TrapMode::Direct);
       // debug!("Kernel TrapHandler func addr    :{:#x}",hander_func);
        //debug!("Traper Set Success!");
    }
}

pub fn set_kernel_forbid(){
    unsafe {
        stvec::write(kernel_traped_forbid as usize, TrapMode::Direct);
    }
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
                let current_trapcx = TASK_MANAER.get_current_trapcx();
                debug!("lat sepc:{:#x}",current_trapcx.sepc_entry_point);
                current_trapcx.x[10] = ret as usize;
            }
        }
        Trap::Exception(Exception::IllegalInstruction)=>{
            panic!("User IllegalInstruction at {:#x}", sepc_val)
        }
        Trap::Exception(Exception::InstructionPageFault)=>{
            error!("User InstructionPageFault at {:#x}, accessing {:#x}", sepc_val, stval_val);
            PageFaultHandler(VirAddr(stval_val));
        }
        Trap::Exception(Exception::LoadPageFault)=>{
            warn!("User LoadPageFault at {:#x}, accessing {:#x}", sepc_val, stval_val);
            PageFaultHandler(VirAddr(stval_val));
        }
        Trap::Exception(Exception::StorePageFault)=>{
            error!("User StorePageFault at {:#x}, accessing {:#x}", sepc_val, stval_val);
            PageFaultHandler(VirAddr(stval_val));
        }
        Trap::Interrupt(Interrupt::SupervisorTimer)=>{
           // print!("time");
            set_next_timeInterupt();
            //error!("timer interrupt");
             //print!("time");
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

pub extern "C" fn kernel_traped_forbid(){//内核专属trap目前只支持时钟设置
    let scauses = scause::read();
    let sepc_val = sepc::read();
    let stval_val = stval::read();
    let sstatus_val = sstatus::read();
    let satp_val = satp::read();

    panic!(
        "UnSupport Kernel Trap: cause={:?} sepc={:#x} stval={:#x} sstatus={:#x} satp={:#x}",
        scauses.cause(),
        sepc_val,
        stval_val,
        sstatus_val.bits(),
        satp_val.bits()
    )

}

global_asm!(include_str!("trap.asm"));
