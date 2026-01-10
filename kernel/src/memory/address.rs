use core::ops::Add;
use core::sync::atomic::{compiler_fence,Ordering};
use bitflags::bitflags;
use log::{debug, error, trace, warn};
use riscv::register::satp;
use crate::memory::MapArea;
use crate::{config::*, memory::frame_allocator::*};
use alloc::vec::Vec;
use alloc::vec;
#[derive(Debug,Clone,Copy,PartialEq, Eq, PartialOrd, Ord)]
pub struct VirNumber(pub usize); //虚拟页号
#[derive(Debug,Clone,Copy,PartialEq, Eq, PartialOrd, Ord)]
pub struct PhysiNumber(pub usize);
#[derive(Debug,Clone,Copy,PartialEq, Eq, PartialOrd, Ord)]
pub struct VirAddr(pub usize);
#[derive(Debug,Clone,Copy,PartialEq, Eq, PartialOrd, Ord)]
pub struct PhysiAddr(pub usize);
#[derive(Debug,Clone,Copy)]
#[repr(C)]
pub struct PageTableEntry(pub usize);

#[derive(Clone)]
pub struct PageTable{
    pub root_ppn:PhysiNumber,
    entries:Vec<FramTracker>,
}
bitflags! {
    #[derive(Debug)]
    pub struct PTEFlags: usize {
        /// Valid
        const V = 1 << 0;
        /// Readable
        const R = 1 << 1;
        /// Writable
        const W = 1 << 2;
        /// eXecutable
        const X = 1 << 3;
        /// User
        const U = 1 << 4;
        /// Global
        const G = 1 << 5;
        /// Accessed
        const A = 1 << 6;
        /// Dirty
        const D = 1 << 7;
    }
}



impl From<PhysiNumber> for PhysiAddr {
    fn from(value: PhysiNumber) -> Self {
        PhysiAddr(value.0<<PAGE_SIZE_BITS)
    }
}
impl From<VirNumber> for VirAddr {
    fn from(value: VirNumber) -> Self {
        VirAddr(value.0<<PAGE_SIZE_BITS)
    }
}
impl From<PhysiAddr> for PhysiNumber {
    fn from(value: PhysiAddr) -> Self {
        PhysiNumber(value.0>>PAGE_SIZE_BITS)
    }
}
impl From<VirAddr> for VirNumber {
    fn from(value: VirAddr) -> Self {
        VirNumber(value.0>>PAGE_SIZE_BITS)
    }
}



impl VirNumber {
 pub fn index(&self) -> [usize; 3] {
        let  vpn = self.0;
        let mut idx: [usize; 3] = [0; 3];
        // SV39: VPN[2] (最高位) -> VPN[1] -> VPN[0] (最低位)
        idx[0] = (vpn >> 18) & 0x1FF;  
        idx[1] = (vpn >> 9) & 0x1FF;   
        idx[2] = vpn & 0x1FF;
        idx
    }

    ///自身vpn+1
    pub fn step(&mut self)->Self{
        self.0+=1;
        self.clone()
    }
}

impl VirAddr {
    ///向上对齐到页面
    pub fn floor_up(&self)->VirNumber{
        let addr=self.0;
        VirNumber((addr+PAGE_SIZE-1)/PAGE_SIZE)//绝对正确对齐
    }
    ///向下对齐到页面
    pub fn floor_down(&self)->VirNumber{
        let addr=self.0;
        VirNumber(addr/PAGE_SIZE)//直接截断
    }
    pub fn offset(&self)->usize{
        self.0 % PAGE_SIZE
    }
    ///不裁剪对齐转化 要求地址必须对齐
    pub fn strict_into_virnum(&self)->VirNumber{
        if self.0 % PAGE_SIZE!=0{panic!("strict_into_virnum Filed!!");}
        VirNumber(self.0/PAGE_SIZE)
    }
}


impl PhysiAddr{
    ///向上对齐到页面
    pub fn floor_up(&self)->PhysiNumber{
        let addr=self.0;
        PhysiNumber((addr+PAGE_SIZE-1)/PAGE_SIZE)//绝对正确对齐
    }
    ///向下对齐页面
    pub fn floor_down(&self)->PhysiNumber{
        let addr=self.0;
        PhysiNumber(addr/PAGE_SIZE)//直接截断
    }
    ///物理的12位偏移
    pub fn offset(&self)->usize{
        self.0 & (PAGE_SIZE-1) 
    }
}



impl PageTableEntry {
    pub fn new(ppn:usize,flags:PTEFlags)->Self{
        PageTableEntry( (ppn<<10) | flags.bits()) // 页表项不持有frametracer
    }
    pub fn flags(&self) -> PTEFlags {
        PTEFlags::from_bits_truncate(self.0 & 255) //目前跳过2位的rsw保留位
    }
    pub fn set_flags(&mut self,flags:PTEFlags){
        let pte = (self.0 & !(255)) |  flags.bits();
        self.0 = pte;
    }
    pub fn ppn(&self)->PhysiNumber{
         PhysiNumber((self.0 >> 10) & ((1 << 44) - 1))
    }
    pub fn is_valid(&self)->bool{
        let re = self.flags().contains(PTEFlags::V);
        // 刷新页表
        unsafe { riscv::asm::sfence_vma_all(); }
        return re;
    }
    ///设置页表项不合法
    pub fn set_inValid(&mut self){
        self.0=0 //全部置零 
    }
    // 设置脏 刷新页表
    pub fn set_isdirty(&mut self){
        let new_flag = PTEFlags::from_bits_truncate(self.0 & 255) | PTEFlags::D;
        self.set_flags(new_flag);
        // 刷新页表
       // unsafe { riscv::asm::sfence_vma_all(); }
    }
    // 设置访问过 刷新页表
    pub fn set_isaccess(&mut self){
        let new_flag = PTEFlags::from_bits_truncate(self.0 & 255) | PTEFlags::A;
        self.set_flags(new_flag);
        // 刷新页表
       // unsafe { riscv::asm::sfence_vma_all(); }
    }
}

impl PageTable {
    pub fn new()->Self{
        let mut root_frame=alloc_frame().expect("failed to alloc frame for page table");
        PageTable{
            root_ppn:PhysiNumber(root_frame.ppn.0),
            entries:vec![root_frame], //把根页面挂下面 正确，获取所有权
        }
    }

    ///获取内核地址空间的页表视图 只能由内核调用
    pub fn get_kernel_table_layer()->PageTable{
        let inner=KERNEL_SPACE.lock();
        let satp =inner.table.satp_token();
        drop(inner);//drop inner 好习惯
        let table=PageTable{
            root_ppn:PhysiNumber(satp & ((1usize << 44) -1)),
            entries:Vec::new()
        };
        table
    }

    ///根据起始虚拟地址，从satp和vpn和len获取可变的u8数组
    ///通过临时的 PageTable 视图访问用户页表（只用于地址转换）
    pub fn get_mut_slice_from_satp(satp:usize,len:usize,startAddr:VirAddr)->Vec<&'static mut [u8]>{
        let mut start_addr = startAddr;
        let end_addr = VirAddr(start_addr.0 + len);
        // 创建临时页表视图，只用于地址转换，不管理页表生命周期
        let mut table = PageTable::crate_table_from_satp(satp);

        let mut result_v = Vec::new();
        while start_addr < end_addr {
            // 获取当前地址所在的页
            let start_vpn = start_addr.floor_down();
            let source_slice = table.get_mut_byte(start_vpn.into())
                .expect("Get VPN to RealAddr failed");
            // 计算当前页的结束地址
            let mut page_end_addr: VirAddr = VirNumber(start_vpn.0 + 1).into();
            // 取当前页结束地址和总结束地址的最小值
            let real_end_addr = page_end_addr.min(end_addr);
            
            // 计算当前页内需要的字节数
            let start_offset = start_addr.offset();
            let end_offset = if real_end_addr.0 / PAGE_SIZE == start_vpn.0 {
                // 数据在同一页内
                real_end_addr.offset()
            } else {
                // 数据跨页，读取到页面结尾
                PAGE_SIZE
            };
            
            result_v.push(&mut source_slice[start_offset..end_offset]);
            start_addr = real_end_addr;
        }

        result_v
    }

    ///从给定的satp中创建临时新页表 临时使用物理ppn为粗略提取
    pub fn crate_table_from_satp(satp:usize)->Self{
        let table=PageTable{
            root_ppn:PhysiNumber(satp & ((1usize << 44) -1)),
            entries:Vec::new()
        };
        table
    }

 

    ///根据vpn获取该页的可变数组切片,获取从物理页开头的地址切片
    pub fn get_mut_byte(&mut self,vpn:VirNumber)->Option<&'static mut [u8;PAGE_SIZE]>{//防止跨页

        let phydr=self.translate(vpn.into()).expect("Virtual Address translate to Physical Adress Failed!");
        unsafe {
           Some(core::slice::from_raw_parts_mut(phydr.0 as  *mut u8, PAGE_SIZE).try_into().expect("GET_MUT_BYTE FAILED TO TRY TRANSLATE A  POINTER TO STATIC PAGESIZE")) 
        }
    }



    ///专注翻译完整虚拟地址带偏移,结束地址不考虑是否对齐,使用者肯定
    pub fn translate(&mut self,VDDR:VirAddr)->Option<PhysiAddr>{
        
        match self.find_pte_vpn(VDDR.into()){
            Some(pte)=>{
                let ppn=pte.ppn();
                let addr=(ppn.0*PAGE_SIZE)+VDDR.offset();//不考虑是否对齐,使用者肯定
                Some(PhysiAddr(addr))
            }
            None=>{
                None
            }
        }
    }

    ///专注于通过vpn翻译,返回ppn号
    pub fn translate_byvpn(&mut self,vpn:VirNumber)->Option<PhysiNumber>{
        //使用编译器屏障，防止优化内存访问重新排序
        compiler_fence(Ordering::SeqCst);
        match self.find_pte_vpn(vpn.into()){
            Some(pte)=>{
                let ppn=pte.ppn();

                compiler_fence(Ordering::SeqCst);
                Some(ppn)
            }
            None=>{

                None
            }
        }
    }


    pub fn get_pte_array(&self,phynum:usize)->&'static mut [PageTableEntry;512]{//加上长度限制，防止跨界
        let phyaddr:PhysiAddr=PhysiNumber(phynum).into();
        unsafe{core::slice::from_raw_parts_mut(phyaddr.0 as  *mut PageTableEntry, 512).try_into().expect("GET_PET_ARRAY FAILED ,WHEN TRANSLATE A POINTER TO 512 SIZE")}
    }
    
    ///查找但是不创建新页表项 需要检查pte合法
    pub fn find_pte_vpn(&mut self,VirNum:VirNumber)->Option<&mut PageTableEntry>{
        let mut current_ppn=self.root_ppn.0;
        let mut idx=VirNum.index();
        let mut pte_array=self.get_pte_array(current_ppn);

        for (id,index) in idx.iter().enumerate(){
           // 编译器屏障：确保每次循环的内存访问不被优化
            compiler_fence(Ordering::SeqCst);
            let entry=&mut pte_array[*index];
                        
                if id==2{//最后一级
                    return Some(entry);
                }
            if !entry.is_valid(){
                return  None;//不合法
            }
            current_ppn=entry.ppn().0;
            pte_array=self.get_pte_array(current_ppn);
        }

        None
    }

    ///创建vpn到ppn的映射，自动设置pte为合法
    pub fn map(&mut self,vpn:VirNumber,ppn:PhysiNumber,flags:PTEFlags){//map是需要传入对应vpn和ppn的
        let pte=self.find_or_create_pte_vpn(vpn).expect("Failed When Map");

        if pte.is_valid(){

            // 最小补丁，权限合并. 解决当程序段紧凑权限不能覆盖的情况
            pte.set_flags(flags | pte.flags());

            //说明之前已经存在对应的映射了,给个警告级别的提示，因为可能有重叠的
            //warn!("MAP error! vpn:{}has maped before, pte exist ppn:{}",vpn.0,pte.ppn().0);
            return;//返回
        }

        *pte=PageTableEntry::new(ppn.0,flags|PTEFlags::V); //合法
    }

    ///判断该vpn是否存在合法映射
    pub fn is_maped(&mut self,vpn:VirNumber)->bool{//判断对应vpn是否已经被映射过
        match self.find_pte_vpn(vpn){
            Some(pte)=>{
                pte.is_valid() //合法为true代表有有效映射
            }
            None=>{
                false
            }
        }
    }

    ///取消映射，应该和地址空间memset的frametracer联系一起，同时释放对应物理帧  能调用这里说明MapArea的Vpn一定存在
    pub fn unmap(&mut self,vpn:VirNumber){
        //查找对应pte
        let pte=self.find_pte_vpn(vpn);
        match pte{
            Some(pte)=>{
                if pte.is_valid(){                   
                    //second
                    pte.set_inValid();
                }else{
                    error!("This PTE is Invalid");
                }
            }
            None=>{
                error!("Unmap failed!No PTE find to unmap");
            }
        }
    }

    fn find_or_create_pte_vpn(&mut self,VirNum:VirNumber)->Option<&mut PageTableEntry>{
        let mut current_ppn=self.root_ppn.0;
        let mut idx=VirNum.index();
        let mut pte_array=self.get_pte_array(current_ppn);
        for (id,index) in idx.iter().enumerate(){

            let entry=&mut pte_array[*index];
                        
                 if id==2{//最后一级
                    
                    return Some(entry);
                }
            if !entry.is_valid(){
                //不存在页表，开始创建页表
                let frame=alloc_frame().expect("Frame alloc failed on pte alloc");
                let ppn =frame.ppn.0;
                *entry=PageTableEntry::new(ppn, PTEFlags::V);
                self.entries.push(frame);
            }
            current_ppn=entry.ppn().0;
            pte_array=self.get_pte_array(current_ppn);
        }
        None
    }

        ///获取适用于satp的token
    pub fn satp_token(&self)->usize{
          //debug!("token: root ppn:{}", self.root_ppn.0);
          // MODE (8 for Sv39) | ASID (0) | PPN
          (8 << 60) | (self.root_ppn.0)
    }

    
}