use riscv::register::sstatus;
use riscv::register::sstatus::Sstatus;
use riscv::register::sstatus::SPP;
use log::debug;
use crate::no_return_start;
// ============ Trap 上下文 ============
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
