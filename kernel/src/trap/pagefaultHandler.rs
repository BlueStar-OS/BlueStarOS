use log::{debug, error};

use crate::{memory::{PageTable, VirAddr, VirNumRange, VirNumber}, task::TASK_MANAER};





///专门处理非虚拟化环境下的PAGEFAULT exception
///faultVAddr发生fault时被操作的addr
///pagefault触发时的环境可能为内核，可能为用户态 内核态可能是在帮用户处理程序->合法,User态->合法
pub fn PageFaultHandler(faultVAddr:VirAddr){
    debug!("Handle Fault Virtual Address:{:#x}",faultVAddr.0);
    let contain_vpn:VirNumber=faultVAddr.floor_down();
    let tsak_satp=TASK_MANAER.get_current_stap();
    let mut map_layer:PageTable=PageTable::crate_table_from_satp(tsak_satp);//临时的页表视图
    //1.检查这个地址是否合法 是否存在合法页表项 是否有mmap的maparea包含这个地址 不合法格杀勿论,不能造成内核恐慌

    
    match &map_layer.find_pte_vpn(contain_vpn){
        Some(pte)=>{
            //应该是被unmap过了，进一步判断
            if pte.is_valid(){
                //非法!,kail进程
                error!("PTE IS VALID BUT PAGE FAULT,KILLED!");
                TASK_MANAER.kail_current_task_and_run_next();
                return;
            }
        }
        None=>{
            //合法
        }
    }


    //是否有对应area
    let inner=TASK_MANAER.task_que_inner.lock();
    let current=inner.current;
    drop(inner);
    let  inner=TASK_MANAER.task_que_inner.lock();
    //有areacontain并且都是mmap类型的area
    let will_kill :bool={
        let  memset=&mut inner.task_queen[current].lock().memory_set;
        !memset.AallArea_Iscontain_thisVpn(contain_vpn) || !memset.AllArea_NoDefaultType(VirNumRange(contain_vpn,contain_vpn))
    }; 
    if will_kill {
        //没有area包含mmap的地址，杀掉
        error!("area not contain mmap addr kill!");
        drop(inner);//杀任务的话提前drop了
        TASK_MANAER.kail_current_task_and_run_next();
        return;
    }
    
    debug!("ligel!");

    
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