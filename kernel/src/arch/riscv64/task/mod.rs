use core::arch::global_asm;
global_asm!(include_str!("./_switch.S"));

extern "C" {
    pub fn __switch(need_swapout: *const TaskContext, need_swapin: *const TaskContext);
}
#[repr(C)]
#[derive(Clone)]
pub struct TaskContext {
    ra: usize,            //offset 0
    pub kernel_sp: usize, //offser 8
    ///s0-s11 被调用者保存寄存器 switch保存
    calleed_register: [usize; 12], //offset 16-..
}

impl TaskContext {
    /// 创建任务上下文，跳转到 app_entry_point
    /// 注意：kernel_sp 是内核栈指针，不是用户栈！
    /// app_entry_point 是内核函数，需要内核栈来执行
    pub fn return_trap_new(kernel_sp: usize) -> Self {
        extern "C" {
            fn app_entry_point();
        }
        TaskContext {
            ra: app_entry_point as *const () as usize,
            kernel_sp,
            calleed_register: [0; 12],
        }
    }
    ///零初始化
    pub fn zero_init() -> Self {
        TaskContext {
            ra: 0,
            kernel_sp: 0,
            calleed_register: [0; 12],
        }
    }
}
