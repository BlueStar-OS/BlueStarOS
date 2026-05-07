use log::debug;
use riscv::register::sstatus;
use riscv::register::sstatus::Sstatus;
use riscv::register::sstatus::SPP;

pub mod kernel_trap;
pub mod user_trap;

pub use user_trap::kernel_trap_handler;

#[repr(C)]
#[repr(align(8))]
pub struct TrapContext {
    pub x: [usize; 32],
    pub sstatus: Sstatus,
    pub sepc_entry_point: usize,
    pub kernel_satp: usize,
    pub kernel_sp: usize,
    pub trap_handler: usize,
}

pub fn no_return_start() -> ! {
    panic!("Start Function you ret ,WTF????");
}

impl TrapContext {
    pub fn init_app_trap_context(
        entry: usize,
        kernel_satp: usize,
        trap_handler: usize,
        kernel_sp: usize,
        user_sp: usize,
    ) -> Self {
        let mut sstatus = sstatus::read();
        sstatus.set_spp(SPP::User);
        let mut register = [0; 32];
        debug!("SSTATUS:{:#X}", sstatus.bits());
        register[2] = user_sp;
        register[1] = no_return_start as *const () as usize;
        TrapContext {
            x: register,
            sstatus,
            sepc_entry_point: entry,
            kernel_satp,
            kernel_sp,
            trap_handler,
        }
    }
}
