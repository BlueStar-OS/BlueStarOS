// 引入任务切换
use crate::global_asm;
global_asm!(include_str!("./_switch.S"));



// ============ 任务上下文 ============

/// AArch64 任务上下文
/// AArch64 calling convention: x19-x28, x29(fp), x30(lr), sp 需要被调用者保存
#[repr(C)]
#[derive(Clone)]
pub struct TaskContext {
    x19: usize,  // offset 0
    x20: usize,
    x21: usize,
    x22: usize,
    x23: usize,
    x24: usize,
    x25: usize,
    x26: usize,
    x27: usize,
    x28: usize,
    x29: usize,  // fp (frame pointer)
    x30: usize,  // lr (link register) - 返回地址
    pub sp: usize,   // stack pointer
}

impl TaskContext {
    pub fn zero_init() -> Self {
        Self {
            x19: 0, x20: 0, x21: 0, x22: 0, x23: 0,
            x24: 0, x25: 0, x26: 0, x27: 0, x28: 0,
            x29: 0, x30: 0, sp: 0,
        }
    }

    pub fn goto_restore(kstack_ptr: usize) -> Self {
        extern "C" { fn app_entry_point(); }
        Self {
            x19: 0, x20: 0, x21: 0, x22: 0, x23: 0,
            x24: 0, x25: 0, x26: 0, x27: 0, x28: 0,
            x29: 0,  // fp
            x30: app_entry_point as usize,  // lr - 返回地址
            sp: kstack_ptr,
        }
    }
}
