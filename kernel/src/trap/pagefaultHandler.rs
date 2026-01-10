use log::{debug, error, warn};

use crate::{memory::{PTEFlags, PageTable, VirAddr, VirNumRange, VirNumber}, task::TASK_MANAER};
use riscv::register::{scause::{self, Exception, Trap}, sie::Sie, sscratch, sstatus::{self, SPP, Sstatus}, stval, stvec, utvec::TrapMode};
use riscv::register::scause::Scause;

///专门处理非虚拟化环境下的PAGEFAULT exception
///faultVAddr发生fault时被操作的addr
///pagefault触发时的环境可能为内核，可能为用户态 内核态可能是在帮用户处理程序->合法,User态->合法
pub fn PageFaultHandler(faultVAddr:VirAddr,cause:Scause){
    debug!("Handle Fault Virtual Address:{:#x}",faultVAddr.0);
    let contain_vpn:VirNumber=faultVAddr.floor_down();
    let tsak_satp=TASK_MANAER.get_current_stap();
    let mut map_layer:PageTable=PageTable::crate_table_from_satp(tsak_satp);//临时的页表视图

    //1.检查这个地址是否合法 是否存在合法页表项 是否有mmap的maparea包含这个地址 不合法格杀勿论,不能造成内核恐慌
    match &mut map_layer.find_pte_vpn(contain_vpn){
                
        Some(pte)=>{  // 仅仅是路通
            if pte.is_valid() {
                
            }

            // 非法pagefault排除 补AD位 合理非法缺页（mmap并不会设置valid），其它非法
             

            // cpu硬件有权选择不维护 页表 A（access） D(dirty)，需要通知操作系统 
            if pte.is_valid() && pte.flags().contains(PTEFlags::W) && Trap::Exception(Exception::StorePageFault)==cause.cause(){
                // 更新pte的ad位
                (*pte).set_isaccess();
                (*pte).set_isdirty();
                unsafe { riscv::asm::sfence_vma(0, 0) };
                warn!("Update pte access and dirty flags");
                return;
            }else if pte.is_valid() && pte.flags().contains(PTEFlags::R) && (Trap::Exception(Exception::LoadPageFault)==cause.cause() || Trap::Exception(Exception::InstructionPageFault)==cause.cause()) {
                // 更新pte的a位
                (*pte).set_isaccess();
                unsafe { riscv::asm::sfence_vma(0, 0) };
                warn!("Update pte access flags");
                return;
            }
            else if !pte.is_valid(){
                // 继续pagefault路程
            }else {
                //非法!,kail进程
                error!("PageFault Unhandled! Killed.");
                error!("  Addr: {:#x}", faultVAddr.0);
                error!("  Cause: {:?}", cause.cause());
                error!("  PTE Flags: {:?}", pte.flags());
                error!("  - Valid: {}", pte.is_valid());
                error!("  - Readable: {}", pte.flags().contains(PTEFlags::R));
                error!("  - Writable: {}", pte.flags().contains(PTEFlags::W));
                error!("  - Dirty: {}", pte.flags().contains(PTEFlags::D));
                TASK_MANAER.kail_current_task_and_run_next();
                return;
            }
        }
        None=>{ // 路不通
            //合法
        }
    }


    //是否有对应area
    let inner=TASK_MANAER.task_que_inner.lock();
    let current=inner.current;
    drop(inner);
    let  inner=TASK_MANAER.task_que_inner.lock();
    // 必须有 area 包含该 vpn，且该 area 是 mmap 区域（memset.areas 中 MapArea.mmap.is_some()）。
    let will_kill :bool={
        let memset=&mut inner.task_queen[current].lock().memory_set;
        !memset.is_mmap_vpn(contain_vpn)
    }; 
    if will_kill {

        //没有area包含mmap的地址，杀掉
        error!("area not contain mmap addr kill!");
        drop(inner);//杀任务的话提前drop了
        TASK_MANAER.kail_current_task_and_run_next();
        return;
    }
    
    debug!("[PageFaultHandler]:ligel!");

    
    {
        //重新拿锁
        let memset=&mut inner.task_queen[current].lock().memory_set;

        //合法，然后
        //2.分配物理页帧挂载到对应的maparea下面
        //3.设置合法页表项
        //一部到位
        
        memset.findarea_allocFrame_and_setPte(contain_vpn);
    }

    
    //返回 释放inner
    drop(inner);

}   