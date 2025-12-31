use alloc::collections::btree_map::BTreeMap;
use bitflags::bitflags;
use alloc::vec::Vec;
use log::{debug, error, trace};
use riscv::paging::PTE;
use core::arch::asm;
    use riscv::register::satp;
    use crate::task::file_loader;

use crate::{config::*, memory::{address::*, alloc_frame, frame_allocator::FramTracker}};
use crate::trap::no_return_start;
use crate::trap::TrapFunction;
 use lazy_static::lazy_static;
 use crate::sync::UPSafeCell;
///开始和结束，一个范围,自动[start,end] start地址自动向下取整，end也向下取整，因为virnumrange用于代码映射，防止代码缺失, startva/PAGE =num+offset ,从num开始，endva/pagesize=endva+offset由于闭区间所以向下取整,防止多映射
#[derive(Debug,Clone, Copy)]
pub struct VirNumRange(pub VirNumber,pub VirNumber);
bitflags! {//MapAreaFlags 和 PTEFlags 起始全为0
    pub struct MapAreaFlags: usize {
        ///Readable
        const R = 1 << 1;
        ///Writable
        const W = 1 << 2;
        ///Excutable
        const X = 1 << 3;
        ///Accessible in U mode
        const U = 1 << 4;  //这里是maparea的标志 不要和页表标志搞混淆
    }
}

///VirNumRange迭代器类型
pub struct VirNumRangeIter{
    current:VirNumber,
    end:VirNumber
}

 pub struct KernelStackAllocator {
     current_id: usize,
     recycle: Vec<usize>,
 }

 impl KernelStackAllocator {
     fn new() -> Self {
         Self { current_id: 0, recycle: Vec::new() }
     }

     fn alloc_id(&mut self) -> usize {
         if let Some(id) = self.recycle.pop() {
             id
         } else {
             let id = self.current_id;
             self.current_id += 1;
             id
         }
     }
 }

 lazy_static! {
     static ref KERNEL_STACK_ALLOCATOR: UPSafeCell<KernelStackAllocator> = unsafe {
         UPSafeCell::new(KernelStackAllocator::new())
     };
 }
impl Iterator for VirNumRangeIter {
    type Item = VirNumber;
    fn next(&mut self) -> Option<Self::Item> {
        let mut current=self.current;
        let end =self.end;
        if current.0<=end.0{
            let cur=current.0;
            debug!("cur:{}",cur);
            self.current.step();
            return Some(VirNumber(cur));
        }else {
            return None;
        }
    }
}

impl IntoIterator for VirNumRange {
    type IntoIter = VirNumRangeIter;
    type Item = VirNumber;
    fn into_iter(self) -> Self::IntoIter {
        VirNumRangeIter{
            current:self.0,
            end:self.1
        }
    }
}

impl VirNumRange {
    ///VirNumRange初始化 传入起始地址和结束地址,闭区间都需要映射 [start,end] start地址自动向下取整，end也向下取整
    pub fn new(start:VirAddr,end:VirAddr)->Self{
        let start_vpn=start.floor_down();
        let end_vpn=end.floor_down();
        VirNumRange(start_vpn, end_vpn)//闭区间，都需要映射
    }
    ///查找区间是否包含某个vpn号 自身是闭区间
    pub fn is_contain_thisvpn(&self,vpn:VirNumber)->bool{
        let start=self.0;
        let end =self.1;
        //闭区间
        if vpn>=start && vpn<=end{
            return true;
        }else {
            return false;
        }
    }

    ///查找区间是和这个区间有交集 自身是闭区间
    pub fn is_contain_thisvpnRange(&self,vpnRange:VirNumRange)->bool{
        let start=self.0;
        let end =self.1;
        let target_start=vpnRange.0;
        let target_end = vpnRange.1;
        //闭区间
        if (target_start>=start && target_start<=end )| (target_end>=start && target_end<=end){
            return true;
        }else {
            return false;
        }
    }
}

impl From<MapAreaFlags> for PTEFlags {
    fn from(value: MapAreaFlags) -> Self {
        match PTEFlags::from_bits(value.bits()){
            Some(pteflags)=>{pteflags}
            None=>{panic!("MapAreaFlags translate to PTEFlags Failed!")}
        }
    }
}
#[derive(PartialEq,Clone, Copy,Debug)]
pub enum MapType {
    Indentical,//直接分配页帧
    Maped,//不直接分配页帧
}
#[derive(Debug,Clone)]
pub struct MapArea{
    ///虚拟页号范围,闭区间
    range:VirNumRange,
    flags:MapAreaFlags,//访问标志   
    pub frames:BTreeMap<VirNumber,FramTracker>,//Maparea 持有的物理页
    map_type:MapType,
    area_type:MapAreaType
}
#[derive(Debug,Clone, Copy,PartialEq, Eq)]
///用户程序如果MMAP 只能 maped映射
pub enum MapAreaType {
    ///有实际挂载的物理页帧和对应页表项，或者恒等映射有明确的物理页帧映射对象的
    DEFAULT,
    ///只是预留虚拟地址空间，没有合法页表项，目前没有对应物理页帧，后期pagefault处理，目的只是检测访问pagefault的地址是否为先前映射的
    MMAP,
}

#[derive(Clone)]
pub struct MapSet{
    ///页表
    pub table:PageTable,
    areas:Vec<MapArea>,
}
impl MapArea {
    ///range,闭区间
    pub fn new(range:VirNumRange,flags:MapAreaFlags,map_type:MapType,area_type:MapAreaType)->Self{
        MapArea{
            range,
            flags,
            frames:BTreeMap::new(),
            map_type,
            area_type,
        }
    }

    ///自身mapareatype 是不是这个
    pub fn areatype_is_this(&self,this:MapAreaType)->bool{
        if this == self.area_type{
            return true;
        }else {
            return false;
        }
    }
    

    pub fn map_one(&mut self,vpn:VirNumber,page_table:&mut PageTable){//带自动分配物理页帧的
        //可能是恒等和普通映射
        let ppn:PhysiNumber;
        if page_table.is_maped(vpn){return;}//如果映射过了就跳过,防止多个一个vpn对应多个ppn，但是只有最后的ppn有效
        match self.map_type{
            MapType::Indentical=>{
               // trace!("Identical map");
                ppn =PhysiNumber(vpn.0) //内核特权高大上，恒等映射 内核映射所有物理帧，但是不能占用和分配对应Framtracer，需要构建一个特殊页表
            }
            MapType::Maped=>{
               let frame= alloc_frame().expect("Memory Alloc Failed By map_one");
                ppn=frame.ppn;
                self.frames.insert(vpn,frame ); //管理最终pte对应的frametracer，分工明确 巧妙！！！！
                trace!("map vpn:{}->ppn:{}",vpn.0,ppn.0)
            }
        };
        page_table.map(vpn, ppn, self.flags.into());
        //debug!("Map Aread map vpn:{} -> ppn:{}",vpn.0,ppn.0);
    }

    ///映射分割和挂载MapArea所有段,闭区间全部映射
    pub fn map_all(&mut self,page_table:&mut PageTable){
        let start=self.range.0;
        let end=self.range.1;
        let mut current=start;
        while current.0<=end.0 {
            self.map_one(current, page_table);
            current.0+=1;
        }

    }

    ///通过虚拟页号释放一个页帧
    pub fn unmap_one(&mut self,table:&mut PageTable,vpn:VirNumber){
        if self.frames.contains_key(&vpn){
            self.frames.remove(&vpn.clone()).expect("Remove a exist vpn failed!!");//回收页帧
            table.unmap(vpn);
        }else{
            error!("MapArea try Unmap vpn:{} but not find vpn in this area",vpn.0);
        }
    }

    ///释放maparea所有页帧
    pub fn unmap_all(){

    }


    ///复制MAPED映射的数据到物理页帧,maped方式才调用它(不包含判断)  必须按照elf格式的顺序复制,传入的data需要自行截断，有栈等映射不需要复制数据
    pub fn copy_data(&mut self,data:Option<&[u8]>,table:&mut PageTable){
        let mut start: usize = 0;
        let mut current_vpn = self.range.0;
        if let None =data{return;}//不需要复制数据的话就返回了
        let len = data.expect("No data").len();
        loop {
            let src = &data.expect("No data")[start..len.min(start + PAGE_SIZE)];
            let dst =&mut table.get_mut_byte(current_vpn).expect("Cant get mut slice")[..src.len()];
            dst.copy_from_slice(src);
            start += PAGE_SIZE;
            if start >= len {
                break;
            }
            current_vpn.step();
        }
    }
    
    
}
impl MapSet {

    ///复制Mapset 解析每一个maparea的页表，申请新页然后将数据搬过去
    pub fn clone_mapset(&mut self)->Self{
        // 目标：为 fork 复制一份“独立”的地址空间。
        // - DEFAULT + MAPED：逐页分配新物理页帧，建立相同的页表映射，并复制页内容
        // - DEFAULT + INDENTICAL：建立相同的恒等映射（不分配新页帧）
        // - MMAP：只复制虚拟地址空间的“预留信息”（MapArea 元数据），不建立页表项、不分配物理页
        //
        // 注意：当前实现是“全量拷贝”（不是 COW），所以父子进程互不影响。

        // 1) 创建一个空的 MapSet，先把 trap 映射补齐（trap 映射不是通过 MapArea 管理的）
        let mut new_set = MapSet::new_bare();
        new_set.map_traper();

        // 2) 逐个克隆 MapArea
        for area in self.areas.iter() {
            // 复制一份 MapArea 的元信息（range/flags/map_type/area_type）
            let mut new_area = MapArea::new(area.range, area.flags, area.map_type, area.area_type);

            match area.area_type {
                MapAreaType::MMAP => {
                    // MMAP 类型只预留地址空间：不映射、不分配页帧
                }
                MapAreaType::DEFAULT => {
                    match area.map_type {
                        MapType::Indentical => {
                            // 恒等映射：直接建立相同 vpn->ppn(vpn) 映射即可
                            // 这种映射一般用于内核空间或特殊区域（用户 MapSet 中通常较少）
                            let start = area.range.0;
                            let end = area.range.1;
                            let mut vpn = start;
                            while vpn.0 <= end.0 {
                                // MapType::Indentical 时 ppn 就是 vpn
                                new_set.table.map(vpn, PhysiNumber(vpn.0), area.flags.into());
                                vpn.0 += 1;
                            }
                        }
                        MapType::Maped => {
                            // 普通映射：逐页分配新帧并复制页内容
                            let start = area.range.0;
                            let end = area.range.1;
                            let mut vpn = start;
                            while vpn.0 <= end.0 {
                                // 如果父进程页表中该页并没有合法映射（例如 MMAP 尚未触发缺页），则跳过
                                if !self.table.is_maped(vpn) {
                                    vpn.0 += 1;
                                    continue;
                                }

                                // 2.1 在新地址空间中分配页帧并建立页表项
                                // new_area.map_one 会：alloc_frame + new_area.frames.insert + new_set.table.map
                                new_area.map_one(vpn, &mut new_set.table);

                                // 2.2 拷贝父进程该页的内容到子进程
                                // 这里直接按 PAGE_SIZE 全页拷贝（ELF 段尾的空洞也会被拷贝为 0/原值）
                                let src = self.table.get_mut_byte(vpn)
                                    .expect("clone_mapset: src page not mapped");
                                let dst = new_set.table.get_mut_byte(vpn)
                                    .expect("clone_mapset: dst page not mapped");
                                dst.copy_from_slice(src);

                                vpn.0 += 1;
                            }
                        }
                    }
                }
            }

            new_set.areas.push(new_area);
        }

        new_set
    }
    

    ///获取当前memset的table临时借用
    pub fn get_table(&mut self)->&mut PageTable{
        &mut self.table    
    }


    ///查找这个vpn对应的area 给这个vpn的maparea分配物理帧，添加合法页表映射 前提是检查过确实有area包含vpn
    pub fn findarea_allocFrame_and_setPte(&mut self,vpn:VirNumber){
        let index = self.areas.iter().position(|area|{
            area.range.is_contain_thisvpn(vpn)
        }).expect("Logim ");
        let area=&mut self.areas[index];
        debug!("Find Map Area! vpn:{} ",vpn.0);
        area.map_one(vpn, &mut self.table);//mmap类型的area也是maped不可能存在恒等映射的用户程序
    }


    ///mmap系统调用，创建一个有vpnrange的maparea，没有实际映射条目和物理页帧的maparea 
    ///startVAR mmap起始地址 size:映射长度(会被裁剪，小于一个页映射一个页,不满一个页补全一个页) 返回-1代表失败 0代表成功 
    pub fn mmap(&mut self,startVAR:VirAddr,size:usize)->isize{
        let start_vpn:VirNumber=startVAR.floor_down();
        let end_vpn:VirNumber=VirAddr(startVAR.0+size-1).floor_down();//就是flourdown
        let range:VirNumRange=VirNumRange(start_vpn, end_vpn);
        //映射地址合法性检查（从开始到结束的区间是否和当前有交集）
        //1.受否已经存在对应vpn项
        if self.AallArea_Iscontain_thisVpn_plus(range){
            return -1;
        }
        //记得加用户U权限
        let mapflags=MapAreaFlags::R | MapAreaFlags::W | MapAreaFlags::X | MapAreaFlags::U;//默认可读写可执行（兼容后面的换入换出）
        //没有对应vpn，在该个mapset就没有对应的映射。之前存在并且unmap时应该处理或销毁其对应页表项，所有这里合法，支持!
        self.add_area(range, MapType::Maped, mapflags, None, MapAreaType::MMAP);
        0
    }

    ///unmap系统调用,取消映射一个[start,end]范围的虚拟页面，并且设置对应页表项不合法
    /// startVAR mmap起始地址 size:映射长度(会被裁剪，小于一个页取消映射一个页,不满一个页补全一个页) 返回-1代表失败 0代表成功 
    pub fn unmap_range(&mut self,startVAR:VirAddr,size:usize,)->isize{
        //合法性检查，是否之前有过映射
        let start_vpn:VirNumber=startVAR.floor_down();
        let end_vpn:VirNumber=VirAddr(startVAR.0+size-1).floor_down();//就是flourdown
        let range:VirNumRange=VirNumRange(start_vpn, end_vpn);
        if !self.AallArea_Iscontain_thisVpn_plus(range){//没有映射不能取消映射
            return -1;
        }
        //剩下是有映射的了，但是得判断是不是MMAP类型的area，不能取消映射DEFAULTD段
        //找存在映射的area判断所有是否是default
        if !self.AllArea_NoDefaultType(range){
            return -1;
        }
        debug!("nocontain default type area,and  exits:{:#x} flect previous",startVAR.0);
        //也没有defalut的area，可以取消映射
        //1.非法对应页表项 MMAP area有可能没有触发过缺页，就没有对应pte
        for num in range{
            match self.table.find_pte_vpn(num){
                Some(mut pte)=>{
                debug!("find mmaped pte will set it illegal! vpnnum:{}",num.0);
                  pte.set_inValid();  
                }
                None=>{

                }
            }
        }

        //2.移除对应area 需要查找所有关联的maparea，因为可能是多次
        let match_vec= self.pop_contain_range_area(range);
        debug!("Find {} maparea connect",match_vec.len());
        //3.判断结果是否为空
        if match_vec.is_empty(){
            panic!("Unmap logim error");//是有还是没有，逻辑严重不符
            return -1;
        }
        trace!("Unmap Area:{:?}",match_vec);

        //成功 maparea之后会free~~~~，页帧会自动释放
        0
    }



    ///从elf解析数据创建应用地址空间 Mapset entry user_stack,kernel_sp
    /// elf_data: ELF 文件数据（可以从文件系统读取）
    pub fn from_elf(elf_data:&[u8])->(Self,usize,VirAddr,usize){ 
        let mut memory_set = Self::new_bare();
        // map program headers of elf, with U flag
        let elf = xmas_elf::ElfFile::new(elf_data).unwrap();
        let elf_header = elf.header;
        let magic = elf_header.pt1.magic;
        assert_eq!(magic, [0x7f, 0x45, 0x4c, 0x46], "invalid elf!");
        let ph_count = elf_header.pt2.ph_count();
        let mut max_end_vpn = VirNumber(0);//为elf结尾所在段+1
        let entry_point = elf.header.pt2.entry_point();
        debug!("ELF entry point: {:#x}, program headers: {}", entry_point, ph_count);
        for i in 0..ph_count {
            let ph = elf.program_header(i).unwrap();
            if ph.get_type().unwrap() == xmas_elf::program::Type::Load {
                let start_va: VirAddr = VirAddr(ph.virtual_addr() as usize);
                let end_va: VirAddr = VirAddr((ph.virtual_addr() + ph.mem_size()) as usize);
                let mut map_perm = MapAreaFlags::U;
                let ph_flags = ph.flags();
                if ph_flags.is_read() {
                    map_perm |= MapAreaFlags::R;
                }
                if ph_flags.is_write() {
                    map_perm |= MapAreaFlags::W;
                }
                if ph_flags.is_execute() {
                    map_perm |= MapAreaFlags::X;
                }
                
                debug!("  [{}] Mapping segment: [{:#x}, {:#x}), perm: {:?}", 
                       i, start_va.0, end_va.0, map_perm);
                
                max_end_vpn=end_va.floor_up();
                memory_set.add_area(VirNumRange::new(start_va, end_va),
                 MapType::Maped,
                  map_perm,
                  Some(&elf.input[ph.offset() as usize..(ph.offset() + ph.file_size()) as usize]),
                  MapAreaType::DEFAULT );//应用area默认default
            }
        }
        
        //程序地址空间创建完成，接下来是
        // 映射陷阱
        memory_set.map_traper();
        //映射上下文
        memory_set.map_trapContext();
        //映射普通用户栈
        let userstack_start_vpn=VirNumber(max_end_vpn.0+1);//留guradpage
        let userstack_end_vpn=VirNumber(userstack_start_vpn.0+1);
        let user_sp:VirAddr=VirAddr(userstack_end_vpn.0*PAGE_SIZE + PAGE_SIZE);//因为结尾不包含，属于下一个页面
        debug!("  Mapping user stack: vpn={:#x}, sp={:#x}", userstack_start_vpn.0, user_sp.0);
        memory_set.add_area(VirNumRange(userstack_start_vpn,userstack_end_vpn), 
        MapType::Maped,
         MapAreaFlags::W | MapAreaFlags::R | MapAreaFlags::U,
          None
        ,MapAreaType::DEFAULT);
        //映射用户堆
        let userheap_start_end_vpn = VirNumber(userstack_start_vpn.0+1);//无需guardpage，堆不会向下溢出
        debug!("  Mapping user heap: vpn={:#x}", userheap_start_end_vpn.0);
        memory_set.add_area(VirNumRange(userheap_start_end_vpn, userheap_start_end_vpn),
         MapType::Maped, 
         MapAreaFlags::R | MapAreaFlags::W | MapAreaFlags::U, 
         None,
        MapAreaType::DEFAULT);

        // 分配并映射内核栈（使用全局分配器，不再依赖 appid）
        let kernel_stack_top = Self::alloc_kernel_stack();
        (
            memory_set,
            entry_point as usize,
            user_sp,
            kernel_stack_top
        )
    }

    pub(crate) fn alloc_kernel_stack() -> usize {
        // 分配一个逻辑 id（从 1 开始），用于在高地址区域切分出一段内核栈虚拟地址区间
        let id = KERNEL_STACK_ALLOCATOR.lock().alloc_id() + 1;

        // 高地址向下切：TRAP_BOTTOM_ADDR 之下为多个 task kernel stack，每段栈前留一个 guard page
        let strat_kernel_vpn = VirAddr(TRAP_BOTTOM_ADDR - (PAGE_SIZE + KERNEL_STACK_SIZE) * id)
            .strict_into_virnum();
        let end_kernel_vpn = VirAddr(
            TRAP_BOTTOM_ADDR - ((PAGE_SIZE + KERNEL_STACK_SIZE) * id) + KERNEL_STACK_SIZE - PAGE_SIZE,
        )
        .strict_into_virnum();
        let kernel_stack_top =
            TRAP_BOTTOM_ADDR - ((PAGE_SIZE + KERNEL_STACK_SIZE) * id) + KERNEL_STACK_SIZE;

        KERNEL_SPACE.lock().add_area(
            VirNumRange(strat_kernel_vpn, end_kernel_vpn),
            MapType::Maped,
            MapAreaFlags::R | MapAreaFlags::W,
            None,
            MapAreaType::DEFAULT,
        );

        kernel_stack_top
    }


    pub(crate) fn new_bare()->Self{
        MapSet{
            table:PageTable::new(),
            areas:Vec::new(),
        }
    }

    ///在目前的地址空间页表里面映射陷阱
    pub fn map_traper(&mut self,){
        let kernel_trape:usize=straper as usize;//内核陷阱起始物理地址
        self.table.map(VirAddr(TRAP_BOTTOM_ADDR).into(), PhysiAddr(kernel_trape as usize).into(), PTEFlags::X | PTEFlags::R);

       
    }

    ///映射陷阱上下文
    pub fn map_trapContext(&mut self){
        let trapcontext_addr:VirAddr = VirAddr(TRAP_CONTEXT_ADDR);
        self.add_area(
            VirNumRange(trapcontext_addr.strict_into_virnum(), 
            trapcontext_addr.strict_into_virnum()), 
            MapType::Maped, MapAreaFlags::R | MapAreaFlags::W, 
            None
            ,MapAreaType::DEFAULT);
    }

    ///目前不可用
    ///映射特殊用户库没返回的情况，可以直接切换任务或者panic，保证内核稳定,目前就在TrapContext后面巴，如果后续报错，则需要特殊处理。！！！！！！！！！！！！！！！！！！！！！！
    ///只映射了处理函数一个页，可能不够 目前不能用
   // pub fn map_user_start_return(&mut self){
     //   let userlib_start_retunr:usize=USERLIB_START_RETURN_HIGNADDR;
       // let map_vnumber=VirAddr(userlib_start_retunr).strict_into_virnum();//严格对齐
        //let start_return_phyaddr =PhysiAddr(no_return_start as usize).floor_down();
        //self.table.map(map_vnumber, start_return_phyaddr, PTEFlags::U | PTEFlags::X | PTEFlags::R);//用户唯一可以访问的高地址

   // }

    ///判断自身的所有maparea是否有过对应vpn的映射或者mmap,只能检查一个页面
    /// vpn ：需要查找的vpn虚拟页号.
    pub fn AallArea_Iscontain_thisVpn(&self,vpn:VirNumber)->bool{
        self.areas.iter().any(|area|{
            area.range.is_contain_thisvpn(vpn)
        })

    }

    ///判断自身的所有maparea是否有过对应vpn的映射或者mmap,求是否存在交集
    /// VpnRange ：连续闭区间，需要查找的vpn虚拟页号范围.
    pub fn AallArea_Iscontain_thisVpn_plus(&self,vpnrange:VirNumRange)->bool{
        self.areas.iter().any(|area|{
            area.range.is_contain_thisvpnRange(vpnrange)
        })
    }

    ///判断这个范围内的area有DEFAULT的映射方式吗
    pub fn AllArea_NoDefaultType(&self,range:VirNumRange)->bool{
        //首先找到哪些area包含range里面的vpn
        self.areas.iter().any(|area|{
            area.areatype_is_this(MapAreaType::DEFAULT)
        })
    }

    ///获取所有包含范围内vpn的maparea的实体move所有权
    pub fn pop_contain_range_area(&mut self,range:VirNumRange)->Vec<MapArea>{
        let mut result:Vec<MapArea>=Vec::new();//存放结果 
        let index:Vec<usize> = self.areas.iter().enumerate().filter(|(_,area)|{
            area.range.is_contain_thisvpnRange(range)
        }).map(|(index,_)|{index}).collect();
        for inde in index{
           result.push(self.areas.remove(inde));
        }
        result
        
    }

    ///输入range，maptype和flags 自动处理maparea的映射和物理帧挂载以及对应memset的pagetable映射,处理数据的复制映射   但是映射用户栈不需要数据
    /// area_type优先级更高 其次map_type
    pub fn add_area(&mut self,range:VirNumRange,map_type :MapType,flags:MapAreaFlags,data:Option<&[u8]>,area_type:MapAreaType){
        let mut area=MapArea::new(range, flags, map_type,area_type);
        match area_type{
            MapAreaType::DEFAULT=>{
                area.map_all(&mut self.table);//映射area,处理物理页帧分配逻辑
                if let MapType::Maped = map_type{//maped方式要复制数据
                    area.copy_data(data, &mut self.table);
                }
            }
            MapAreaType::MMAP=>{
                //啥都不做，mmap目前不用映射和分配物理页帧，留在pagefalut
            }
        }
        self.areas.push(area);
    } 

    pub fn new_kernel()->Self{
        let mut mem_set =MapSet::new_bare();

        //映射陷阱
        mem_set.map_traper();

        //映射硬件段
        let hardware_range = VirNumRange::new(VirAddr(0x0 as usize), VirAddr(0x10010000 as usize));//range封装过
        mem_set.add_area(hardware_range, 
            MapType::Indentical, 
             MapAreaFlags::R | MapAreaFlags::W  ,
             None
            ,MapAreaType::DEFAULT);

        //映射代码段
        let text_range = VirNumRange::new(VirAddr(stext as usize), VirAddr(etext as usize));//range封装过
        mem_set.add_area(text_range, 
            MapType::Indentical, 
             MapAreaFlags::R | MapAreaFlags::X  ,
             None
            ,MapAreaType::DEFAULT);


        //映射rodata段
        let rodata_range = VirNumRange::new(VirAddr(srodata as usize), VirAddr(erodata as usize));//range封装过
        mem_set.add_area(rodata_range,
             MapType::Indentical, 
             MapAreaFlags::R,
             None
            ,MapAreaType::DEFAULT);
        //trace!("{} {}\n",rodata_start_vpn.0,rodata_end_vpn.0);

    
        // 映射内核数据段
        let data_range = VirNumRange::new(VirAddr(sdata as usize), VirAddr(edata as usize));//range封装过
        mem_set.add_area(data_range,
             MapType::Indentical,
              MapAreaFlags::R | MapAreaFlags::W,
              None
            ,MapAreaType::DEFAULT);
       // trace!("{} {}\n",data_start.0,data_end.0);

        //映射bss段
        let bss_range = VirNumRange::new(VirAddr(sbss as usize), VirAddr(ebss as usize));//range封装过
        mem_set.add_area(bss_range,
             MapType::Indentical,
              MapAreaFlags::R | MapAreaFlags::W,
              None,
            MapAreaType::DEFAULT);
       // trace!("{} {}\n",bss_start.0,bss_end.0);
        
        // 映射物理内存(必须手动构造range区间)，phystart需要向上取整,end需要手动-1 range
        let phys_start =VirAddr(ekernel as usize).floor_up();
        let phys_end =VirAddr(ekernel as usize + MEMORY_SIZE-PAGE_SIZE).floor_down(); //ekernel 为结束地址 end需要手动-1 range
        let phys_range = VirNumRange(phys_start,phys_end);
        mem_set.add_area(phys_range, MapType::Indentical,
             MapAreaFlags::W | MapAreaFlags::R,
              None,
            MapAreaType::DEFAULT);
       // trace!("{} {}\n",phys_start.0,phys_end.0);

        //内核地址空间映射完成
        let vdr:VirAddr=phys_end.into();
        debug!("Kernle AddressSet Total Memory:{} MB,Kernel Size:{}MB",(vdr.0 -skernel as usize)/MB,(ekernel as usize -skernel as usize)/MB);
        
        mem_set

    
    }
    
    pub fn translate_test(&mut self){
        self.areas.iter().for_each(|maparea|{
            (maparea.range.0.0..=maparea.range.1.0).for_each(|vpn| {
                let vdr:VirAddr=VirNumber(vpn).into();
                let addr=self.table.translate(vdr);
                debug!("Translate Test vddr:{:#x} ->Phyaddr:{:#x}",vdr.0,addr.unwrap().0)
            });
        } );
    }

    /// Change page table by writing satp CSR Register.
    pub fn activate(&self) {
         let satps = self.table.satp_token();
        debug!("Active PageTable: SATP = {:#x}", satps);
        unsafe {
            satp::write(satps);
            asm!("sfence.vma");
            debug!("Page Witch Successful!!!!!");
        }
    }
}