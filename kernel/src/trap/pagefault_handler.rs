use log::{debug, error, warn};

use crate::arch::memory::*;
use crate::task::TASK_MANAER;
use riscv::register::scause::Scause;
use riscv::register::scause::{Exception, Trap};

/// 打印当前用户态寄存器现场，便于定位“用户态空指针/坏指针到底是谁传出来的”。
/// 需要提前释放锁
fn log_current_user_registers() {
    let trap_context = TASK_MANAER.get_current_trapcx();
    error!(
        "[user-regs] ra={:#x} sp={:#x} gp={:#x} tp={:#x}",
        trap_context.x[1], trap_context.x[2], trap_context.x[3], trap_context.x[4]
    );
    error!(
        "[user-regs] a0={:#x} a1={:#x} a2={:#x} a3={:#x} a4={:#x} a5={:#x} a6={:#x} a7={:#x}",
        trap_context.x[10],
        trap_context.x[11],
        trap_context.x[12],
        trap_context.x[13],
        trap_context.x[14],
        trap_context.x[15],
        trap_context.x[16],
        trap_context.x[17]
    );
    error!(
        "[user-regs] s0={:#x} s1={:#x} s2={:#x} s3={:#x} s4={:#x} s5={:#x} s6={:#x} s7={:#x} s8={:#x} s9={:#x} s10={:#x} s11={:#x}",
        trap_context.x[8],
        trap_context.x[9],
        trap_context.x[18],
        trap_context.x[19],
        trap_context.x[20],
        trap_context.x[21],
        trap_context.x[22],
        trap_context.x[23],
        trap_context.x[24],
        trap_context.x[25],
        trap_context.x[26],
        trap_context.x[27]
    );
    error!(
        "[user-regs] t0={:#x} t1={:#x} t2={:#x} t3={:#x} t4={:#x} t5={:#x} t6={:#x}",
        trap_context.x[5],
        trap_context.x[6],
        trap_context.x[7],
        trap_context.x[28],
        trap_context.x[29],
        trap_context.x[30],
        trap_context.x[31]
    );
}

///专门处理非虚拟化环境下的PAGEFAULT exception
///faultVAddr发生fault时被操作的addr
///pagefault触发时的环境可能为内核，可能为用户态 内核态可能是在帮用户处理程序->合法,User态->合法
pub fn page_fault_handler(fault_vaddr: VirAddr, cause: Scause) {
    // TODO:处理用户栈溢出逻辑

    debug!("Handle Fault Virtual Address:{:#x}", fault_vaddr.0);
    let contain_vpn: VirNumber = fault_vaddr.floor_down();
    let tsak_satp = TASK_MANAER.get_current_stap();
    let mut map_layer: PageTable = PageTable::crate_table_from_satp(tsak_satp); //临时的页表视图

    //1.检查这个地址是否合法 是否存在合法页表项 是否有mmap的maparea包含这个地址 不合法格杀勿论,不能造成内核恐慌
    match &mut map_layer.find_pte_vpn(contain_vpn) {
        Some(pte) => {
            // 仅仅是路通
            pte.is_valid();

            // 非法pagefault排除 补AD位 合理非法缺页（mmap并不会设置valid），其它非法

            // cpu硬件有权选择不维护 页表 A（access） D(dirty)，需要通知操作系统
            if pte.is_valid()
                && pte.flags().contains(PTEFlags::W)
                && Trap::Exception(Exception::StorePageFault) == cause.cause()
            {
                // 更新pte的ad位
                (*pte).set_isaccess();
                (*pte).set_isdirty();
                unsafe { riscv::asm::sfence_vma(0, 0) };
                warn!("Update pte access and dirty flags");
                return;
            } else if pte.is_valid()
                && pte.flags().contains(PTEFlags::R)
                && (Trap::Exception(Exception::LoadPageFault) == cause.cause()
                    || Trap::Exception(Exception::InstructionPageFault) == cause.cause())
            {
                // 更新pte的a位
                (*pte).set_isaccess();
                unsafe { riscv::asm::sfence_vma(0, 0) };
                warn!("Update pte access flags");
                return;
            } else if !pte.is_valid() {
                // 继续pagefault路程
            } else {
                //非法!,kail进程
                error!("PageFault Unhandled! Killed.");
                error!("  Addr: {:#x}", fault_vaddr.0);
                error!("  Cause: {:?}", cause.cause());
                log_current_user_registers();
                error!("  PTE Flags: {:?}", pte.flags());
                error!("  - Valid: {}", pte.is_valid());
                error!("  - Readable: {}", pte.flags().contains(PTEFlags::R));
                error!("  - Writable: {}", pte.flags().contains(PTEFlags::W));
                error!("  - Dirty: {}", pte.flags().contains(PTEFlags::D));
                TASK_MANAER.kail_current_task_and_run_next();
                return;
            }
        }
        None => { // 路不通
             //合法
        }
    }

    //是否有对应area
    let will_kill = TASK_MANAER.task_que_inner.lock(|inner| {
        let current = inner.current;
        inner.task_queen[current].lock(|tcb| !tcb.memory_set.is_mmap_vpn(contain_vpn))
    });

    if will_kill {
        //没有area包含mmap的地址，杀掉
        error!("area not contain mmap addr kill!");
        log_current_user_registers();
        TASK_MANAER.kail_current_task_and_run_next();
        return;
    }

    debug!("[page_fault_handler]:ligel!");

    //合法，分配物理页帧挂载到对应的maparea下面并设置合法页表项
    TASK_MANAER.task_que_inner.lock(|inner| {
        let current = inner.current;
        inner.task_queen[current].lock(|tcb| {
            tcb.memory_set.findarea_alloc_frame_and_set_pte(contain_vpn);
        });
    });
}
