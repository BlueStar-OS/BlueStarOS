use alloc::collections::btree_map::BTreeMap;
use bitflags::bitflags;
use alloc::vec::Vec;
use alloc::sync::Arc;
use crate::memory::memset::satp::Satp;
use alloc::sync::Weak;
use log::warn;
use log::{debug, error, trace};
use core::arch::asm;
use core::cell::RefMut;
use core::hint;
    use riscv::register::satp;
    use crate::fs::vfs::File;
    use crate::task::TaskManagerInner;
    use crate::task::getapp_kernel_sapce;
    use crate::task::{TASK_MANAER, file_loader};

use crate::{config::*, memory::{address::*, alloc_frame, frame_allocator::FramTracker}};
use crate::trap::no_return_start;
use crate::trap::TrapFunction;
 use lazy_static::lazy_static;
 use crate::sync::UPSafeCell;

 lazy_static! {
     static ref NEXT_MMAP_ID: UPSafeCell<u64> = unsafe { UPSafeCell::new(1) };
 }

 fn alloc_mmap_id() -> u64 {
     let mut cur = NEXT_MMAP_ID.lock();
     let id = *cur;
     *cur = cur.saturating_add(1);
     id
 }

 #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
 enum SharedMmapKey {
     Anon { mmap_id: u64, page_index: u64 },
     File { inode_num: u32, file_page: u64 },
 }

 lazy_static! {
     static ref SHARED_MMAP_PAGES: UPSafeCell<BTreeMap<SharedMmapKey, Weak<FramTracker>>> =
         unsafe { UPSafeCell::new(BTreeMap::new()) };
 }
///开始和结束，一个范围,自动[start,end] start地址自动向下取整，end也向下取整，因为virnumrange用于代码映射，防止代码缺失, startva/PAGE =num+offset ,从num开始，endva/pagesize=endva+offset由于闭区间所以向下取整,防止多映射
#[derive(Debug,Clone, Copy)]
pub struct VirNumRange(pub VirNumber,pub VirNumber);
bitflags! {//MapAreaFlags 和 PTEFlags 起始全为0
    #[derive(Debug,Clone, Copy)]
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

bitflags! {
    #[derive(Debug,Clone, Copy)]

    pub struct MmapProt: usize {
        const READ = 0x1;
        const WRITE = 0x2;
        const EXEC = 0x4;
    }
}

bitflags! {
    #[derive(Debug,Clone, Copy)]
    pub struct MmapFlags: usize {
        const SHARED = 0x01;
        const PRIVATE = 0x02;
        const FIXED = 0x10;
        const ANONYMOUS = 0x20;
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

    ///左端点
    pub fn left_point(&self)->VirNumber{
        self.0
    }

    ///右端点
    pub fn right_point(&self)->VirNumber{
        self.1
    }

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
    pub fn is_contain_thisvpnRange(&self, vpnRange: VirNumRange) -> Vec<VirNumber> {
        let start = self.0;
        let end = self.1;
        let target_start = vpnRange.0;
        let target_end = vpnRange.1;

        let inter_start = if start >= target_start { start } else { target_start };
        let inter_end = if end <= target_end { end } else { target_end };

        if inter_start > inter_end {
            return Vec::new();
        }

        let mut result = Vec::new();
        for vpn in VirNumRange(inter_start, inter_end) {
            result.push(vpn);
        }
        result
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

#[derive(Clone)]
pub struct MmapInfo {
    pub id: u64,
    pub flags: MmapFlags,
    pub prot: MmapProt,
    pub backing: Option<Arc<dyn File>>,
    pub offset: usize,
}

#[derive(Clone)]
pub struct MapArea{ //通常为单次push进来，虽然粒度大，保证push粒度足够小即可
    ///虚拟页号范围,闭区间
    range:VirNumRange,
    flags:MapAreaFlags,//访问标志   
    pub frames:BTreeMap<VirNumber,Arc<FramTracker>>,//Maparea 持有的物理页
    map_type:MapType,
    /// Some => this area is created by mmap syscall (lazy allocation / file-backed / shared semantics).
    pub mmap: Option<MmapInfo>,
}

#[derive(Clone)]
pub struct MapSet{
    ///页表
    pub table:PageTable,
    areas:Vec<MapArea>,
    pub brk:VirAddr, //进程brk点
}
impl MapArea {
    





    ///range,闭区间
    pub fn new(range:VirNumRange,flags:MapAreaFlags,map_type:MapType)->Self{
        MapArea{
            range,
            flags,
            frames:BTreeMap::new(),
            map_type,
            mmap: None,
        }
    }
    

    /// `first_vpn_ppn` 补丁： 是否已经映射过这个vpn，如果有，请把那个vpn对应的ppnclone一份
    pub fn map_one(&mut self,vpn:VirNumber,page_table:&mut PageTable,first_vpn_ppn:Option<Arc<FramTracker>>){//带自动分配物理页帧的
        //可能是恒等和普通映射
        let ppn:PhysiNumber;
        let is_maped = page_table.is_maped(vpn); // 如果这个vpn已经映射并且合法，应该让它指向相同的ppn并且权限合并
        match self.map_type{
            MapType::Indentical=>{
               // trace!("Identical map");
                ppn =PhysiNumber(vpn.0) //内核特权高大上，恒等映射 内核映射所有物理帧，但是不能占用和分配对应Framtracer，需要构建一个特殊页表
            }
            MapType::Maped=>{

                if !is_maped {
                    let frame= alloc_frame().expect("Memory Alloc Failed By map_one");
                    ppn=frame.ppn;
                    self.frames.insert(vpn, Arc::new(frame) ); //管理最终pte对应的frametracer，分工明确 巧妙！！！！
                    trace!("map vpn:{}->ppn:{}",vpn.0,ppn.0)
                }else {
                    let last = first_vpn_ppn.expect("Please give last vpn's ppn");
                    // 寻找已经映射的vpn的那个ppn是多少，不能重复分配，但是执行同一个ppn号，也要防止doublefree.这里Arc可以保证
                    ppn = last.ppn;
                    self.frames.insert(vpn, last);
                }
                
            }
        };
        // 权限合并
        page_table.map(vpn, ppn, self.flags.into());
        //debug!("Map Aread map vpn:{} -> ppn:{}",vpn.0,ppn.0);
    }

    pub fn map_one_with_frame(&mut self, vpn: VirNumber, frame: Arc<FramTracker>, page_table: &mut PageTable) {
        if page_table.is_maped(vpn) {
            return;
        }
        let ppn = frame.ppn;
        self.frames.insert(vpn, frame);
        page_table.map(vpn, ppn, self.flags.into());
    }

    ///映射分割和挂载MapArea所有段,闭区间全部映射
    pub fn map_all(&mut self,page_table:&mut PageTable,first_vpn_ppn:Option<Arc<FramTracker>>){
        let start=self.range.0;
        let end=self.range.1;
        let mut current=start;
        while current.0<=end.0 {
            self.map_one(current, page_table,first_vpn_ppn.clone());
            current.0+=1;
        }

    }

    ///通过虚拟页号释放一个页帧
    pub fn unmap_one(&mut self,table:&mut PageTable,vpn:VirNumber){
        if self.frames.contains_key(&vpn){
            self.frames.remove(&vpn.clone()).expect("Remove a exist vpn failed!!");//回收页帧(Arc drop 时真正释放)
            table.unmap(vpn);
        }else{
            error!("MapArea try Unmap vpn:{} but not find vpn in this area",vpn.0);
        }
    }


    ///复制MAPED映射的数据到物理页帧,maped方式才调用它(不包含判断)  必须按照elf格式的顺序复制,传入的data需要自行截断，有栈等映射不需要复制数据
    pub fn copy_data(&mut self, data: Option<(usize, &[u8])>, table: &mut PageTable) {
        if data.is_none() { return; }
        
        // 解构出：页内偏移量(如0x40) 和 源数据切片
        let (mut page_offset, src_data) = data.unwrap();
        
        let mut current_vpn = self.range.0;
        let mut current_src_idx = 0; // 记录源数据已经拷贝了多少字节
        let total_len = src_data.len();

        loop {
            // 1. 计算这一页还剩多少空间可以写 (4096 - offset)
            let available_in_page = PAGE_SIZE - page_offset;
            
            // 2. 计算还剩多少源数据没拷
            let remaining_src = total_len - current_src_idx;
            
            // 3. 决定本次拷贝的长度：取最小值
            let copy_len = available_in_page.min(remaining_src);

            // 如果没数据可拷了，退出
            if copy_len == 0 { break; }

            // 4. 获取目标物理页（整个4096字节）
            let dst_page = table.get_mut_byte(current_vpn).expect("Cant get mut slice");

            // 5. 【关键】源数据：从 current_src_idx 往后取 copy_len 个
            let src = &src_data[current_src_idx .. current_src_idx + copy_len];
            
            // 6. 【关键】目标数据：从 page_offset 往后写 copy_len 个
            let dst = &mut dst_page[page_offset .. page_offset + copy_len];
            
            // 执行拷贝
            dst.copy_from_slice(src);

            // 更新游标
            current_src_idx += copy_len;
            current_vpn.step();
            
            // 重点！除了第一页可能有偏移量，后续所有页都必须从 0 开始写
            page_offset = 0; 
        }
    }
    
    
}

bitflags! {
    pub struct CloneFlags:usize{
        const CSIGNAL            = 0x000000ffusize; // 低 8 位：子进程退出/停止时向父进程发送的信号（如 SIGCHLD）

        const CLONE_VM           = 0x00000100usize; // 共享内存地址空间（线程语义；不共享则类似 fork 的独立地址空间）
        const CLONE_FS           = 0x00000200usize; // 共享文件系统信息（cwd/root/umask 等）
        const CLONE_FILES        = 0x00000400usize; // 共享打开文件表（fd table）
        const CLONE_SIGHAND      = 0x00000800usize; // 共享信号处理器（signal handlers）
        const CLONE_PIDFD        = 0x00001000usize; // 返回 pidfd（较新内核特性）
        const CLONE_PTRACE       = 0x00002000usize; // 让新进程继承被 ptrace 跟踪的状态
        const CLONE_VFORK        = 0x00004000usize; // vfork 语义：父进程阻塞直到子进程 exec/exit
        const CLONE_PARENT       = 0x00008000usize; // 新进程的父进程设为当前进程的父进程（"兄弟" 关系）
        const CLONE_THREAD       = 0x00010000usize; // 同一线程组（共享 TGID；通常需要配合 VM/FILES/SIGHAND）
        const CLONE_NEWNS        = 0x00020000usize; // 新的 mount namespace（挂载命名空间）
        const CLONE_SYSVSEM      = 0x00040000usize; // 共享 System V semaphore undo 列表
        const CLONE_SETTLS       = 0x00080000usize; // 设置 TLS（线程本地存储，如 %fs/%gs 基址）
        const CLONE_PARENT_SETTID= 0x00100000usize; // 在父进程地址空间写入子线程 TID（parent_tidptr）
        const CLONE_CHILD_CLEARTID=0x00200000usize; // 在线程退出时清零 child_tidptr 并做 futex 唤醒
        const CLONE_DETACHED     = 0x00400000usize; // 旧标志：分离线程（历史遗留，现代内核基本忽略）
        const CLONE_UNTRACED     = 0x00800000usize; // 新进程不可被 ptrace 跟踪（或不继承跟踪）
        const CLONE_CHILD_SETTID = 0x01000000usize; // 在子进程地址空间写入自身 TID（child_tidptr）
        const CLONE_NEWCGROUP    = 0x02000000usize; // 新的 cgroup namespace
        const CLONE_NEWUTS       = 0x04000000usize; // 新的 UTS namespace（hostname/domainname）
        const CLONE_NEWIPC       = 0x08000000usize; // 新的 IPC namespace（System V IPC/消息队列等）
        const CLONE_NEWUSER      = 0x10000000usize; // 新的 user namespace（uid/gid 映射）
        const CLONE_NEWPID       = 0x20000000usize; // 新的 PID namespace
        const CLONE_NEWNET       = 0x40000000usize; // 新的 network namespace
        const CLONE_IO           = 0x80000000usize; // 共享 I/O 上下文（ioprio 等）

        const CLONE_CLEAR_SIGHAND= 0x1_0000_0000usize; // 清除共享信号处理器（配合特定 clone 场景，较新/少用）
    }
}

impl MapSet {

    /// 打印mapset每个area的范围和权限
    /// 打印对应页表权限
    pub fn print_area_information(&self){

        let satp = self.table.satp_token();
        let mut tb = PageTable::crate_table_from_satp(satp);

        self.areas.iter().for_each(|area|{
            warn!("Area Viraddr {:#x}-{:#x}
                 \nFlags {:?}",area.range.left_point().0*PAGE_SIZE,area.range.right_point().0*PAGE_SIZE+PAGE_SIZE,area.flags);
            warn!("Every pte:\n");
            for vpn in area.range {
                if let Some(pte) = tb.find_pte_vpn(vpn){
                    warn!("Virnum:{} PTEflags:{:?}",vpn.0,pte.flags());
                }else {
                    warn!("Virnum:{} not exist",vpn.0,);
                };
            }
        });
    }

     /// 如果已经映射这个vpn，就返回这个的frametracker的clonearc
    pub fn find_thisvpn_frame(&self,vpn:VirNumber)->Option<Arc<FramTracker>>{
            //查找已经vpn映射过的ppn 以range.0为目标
            let target = vpn;

            let mut find_re:Option<Arc<FramTracker>> = None;

            self.areas.iter().for_each(|area|{
                let re = area.frames.get_key_value(&target);
                if re.is_some(){
                    find_re = Some(re.unwrap().1.clone())
                }
                
            });
            find_re
    }

    pub fn is_mmap_vpn(&self, vpn: VirNumber) -> bool {
        self.areas.iter().any(|area| area.mmap.is_some() && area.range.is_contain_thisvpn(vpn))
    }

    ///复制Mapset 解析每一个maparea的页表，申请新页然后将数据搬过去
    pub fn clone_mapset(&mut self)->Option<Self>{
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
            // 复制一份 MapArea 的元信息（range/flags/map_type + mmap 元数据）
            let mut new_area = MapArea::new(area.range, area.flags, area.map_type);
            new_area.mmap = area.mmap.clone();

            if area.mmap.is_some() {
                // mmap 区域：只复制虚拟地址空间元数据，不建立页表项、不分配物理页
            } else {
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


                                let re =new_set.find_thisvpn_frame(start);
                                // 2.1 在新地址空间中分配页帧并建立页表项
                                // new_area.map_one 会：alloc_frame + new_area.frames.insert + new_set.table.map
                                new_area.map_one(vpn, &mut new_set.table,re);

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

            new_set.areas.push(new_area);
        }

        Some(new_set)
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
        let statr = self.areas[index].range.left_point();
        let re =self.find_thisvpn_frame(statr);
        let area=&mut self.areas[index];
        debug!("Find Map Area! vpn:{} ",vpn.0);

        if let Some(info) = &area.mmap {
            // mmap area: we do lazy allocation on page fault.
            // - MAP_SHARED: pages may be shared across processes.
            // - MAP_PRIVATE: pages are private to this process (no shared cache here).
            if info.flags.contains(MmapFlags::SHARED) {
                // Shared mapping:
                // 1) Build a stable key identifying the backing object + page index.
                //    - Anonymous MAP_SHARED: key is (mmap_id, page_index) so forked tasks can share.
                //    - File-backed MAP_SHARED: key is (inode_num, file_page) so different fds/processes
                //      mapping the same inode can reuse the same physical page.
                // 2) Lookup a Weak<FramTracker> in global cache; if alive, reuse.
                // 3) On cache miss, allocate a new frame.
                //    - For file-backed: read page content from file into the freshly allocated frame.
                //    - For anonymous: keep the frame zero-filled.
                let page_index = vpn.0.saturating_sub(area.range.0.0) as u64;
                let key = if info.flags.contains(MmapFlags::ANONYMOUS) {
                    SharedMmapKey::Anon { mmap_id: info.id, page_index }
                } else {
                    let backing = match info.backing.as_ref() {
                        Some(v) => v,
                        None => {
                            error!("mmap shared pagefault: missing backing file kill");
                            TASK_MANAER.kail_current_task_and_run_next();
                            return;
                        }
                    };
                    let file_page = (info.offset / PAGE_SIZE) as u64 + page_index;
                    let inode_num = match backing.stat() {
                        Ok(st) => st.inode,
                        Err(_e) => {
                            error!("mmap shared pagefault: stat failed kill");
                            TASK_MANAER.kail_current_task_and_run_next();
                            return;
                        }
                    };
                    SharedMmapKey::File {
                        inode_num,
                        file_page,
                    }
                };

                let shared = SHARED_MMAP_PAGES.lock();
                let existing = shared.get(&key).and_then(|w| w.upgrade());
                drop(shared);

                let frame = match existing {
                    Some(f) => f,
                    None => {
                        let f = Arc::new(alloc_frame().expect("Memory Alloc Failed By shared mmap"));
                        
                        if !info.flags.contains(MmapFlags::ANONYMOUS) {
                            // File-backed MAP_SHARED: populate page content at first touch.
                            // Later faults in other processes will reuse this frame via the shared cache.
                            let file = match info.backing.as_ref() {
                                Some(v) => v,
                                None => {
                                    error!("mmap shared pagefault: missing backing file kill");
                                    TASK_MANAER.kail_current_task_and_run_next();
                                    return;
                                }
                            };
                            let file_off = info.offset.saturating_add((page_index as usize).saturating_mul(PAGE_SIZE));
                            let pa: PhysiAddr = f.ppn.into();
                            let buf = unsafe { core::slice::from_raw_parts_mut(pa.0 as *mut u8, PAGE_SIZE) };
                            match file.read_at(file_off, buf) {
                                Ok(n) => {
                                    if n < PAGE_SIZE {
                                        buf[n..].fill(0);
                                    }
                                }
                                Err(e) => {
                                    error!("mmap shared pagefault: read_at failed off={} err={} kill", file_off, e);
                                    TASK_MANAER.kail_current_task_and_run_next();
                                    return;
                                }
                            }
                        }

                        // Insert as Weak to avoid keeping frames alive forever;
                        // cache entry will naturally expire when the last Arc is dropped.
                        let mut shared = SHARED_MMAP_PAGES.lock();
                        shared.insert(key, Arc::downgrade(&f));
                        f
                    }
                };

                // Map (vpn -> ppn) and remember the Arc in this MapArea.frames.
                area.map_one_with_frame(vpn, frame, &mut self.table);
                return;
            }

            if info.flags.contains(MmapFlags::PRIVATE) && !info.flags.contains(MmapFlags::ANONYMOUS) {
                // Private, file-backed mapping:
                // allocate a fresh frame for this process, then read file content into it.
                // This does NOT use the global shared cache.
                let file = match info.backing.as_ref() {
                    Some(v) => v,
                    None => {
                        error!("mmap private pagefault: missing backing file kill");
                        TASK_MANAER.kail_current_task_and_run_next();
                        return;
                    }
                };

                let frame = Arc::new(alloc_frame().expect("Memory Alloc Failed By private mmap"));
                let file_off = info.offset.saturating_add(((vpn.0.saturating_sub(area.range.0.0)) as usize).saturating_mul(PAGE_SIZE));
                let pa: PhysiAddr = frame.ppn.into();
                let buf = unsafe { core::slice::from_raw_parts_mut(pa.0 as *mut u8, PAGE_SIZE) };
                match file.read_at(file_off, buf) {
                    Ok(n) => {
                        if n < PAGE_SIZE {
                            buf[n..].fill(0);
                        }
                    }
                    Err(e) => {
                        error!("mmap private pagefault: read_at failed off={} err={} kill", file_off, e);
                        TASK_MANAER.kail_current_task_and_run_next();
                        return;
                    }
                }

                area.map_one_with_frame(vpn, frame, &mut self.table);
                return;
            }
        }

         

        // Fallback:
        // - Anonymous MAP_PRIVATE mmap (or any other mmap area not handled above): allocate a fresh frame.
        // - Non-mmap areas: should usually already be mapped; but if we get here, keep old behavior.
        area.map_one(vpn, &mut self.table,re);
    }




    fn range_is_free(&self, start: usize, len: usize) -> bool {
        if len == 0 {
            return false;
        }
        let end = start.saturating_add(len).saturating_sub(1);
        let start_vpn = VirAddr(start).floor_down();
        let end_vpn = VirAddr(end).floor_down();
        !self.AallArea_Iscontain_thisVpn_plus(VirNumRange(start_vpn, end_vpn))
    }

    fn find_free_range(&self, len: usize) -> Option<usize> {
        if len == 0 {
            return None;
        }
        let len_align_page_viraddr:VirAddr = VirAddr(len).floor_down().into();
        let len:usize = len_align_page_viraddr.0;
        
        // Minimal policy:
        // - start searching from a page-aligned brk
        // - keep a gap below TRAP_CONTEXT
        let cur_align_page_viraddr:VirAddr = VirAddr(len).floor_up().into();
        let mut cur = cur_align_page_viraddr.0;
        let upper = TRAP_CONTEXT_ADDR.saturating_sub(PAGE_SIZE);

        while cur.saturating_add(len) <= upper {
            if self.range_is_free(cur, len) {
                return Some(cur);
            }
            cur = cur.saturating_add(PAGE_SIZE);
        }
        None
    }


    ///mmap系统调用，创建一个有vpnrange的maparea，没有实际映射条目和物理页帧的maparea 
    /// Linux/POSIX: mmap(addr, len, prot, flags, fd, offset)
    /// 返回：成功返回映射起始地址；失败返回 -1
    pub fn mmap(&mut self, addr: VirAddr, len: usize, prot: usize, flags: usize, fd: i32, offset: usize, fd_backing: Option<Arc<dyn File>>) -> isize {
        //warn!("enter memset mmap");
        if len == 0 {
            return -1;
        }
        if offset % PAGE_SIZE != 0 {
            return -1;
        }

        // Reject integer overflow on offset+len (file-backed) and on addr+len (fixed address).
        if offset.checked_add(len).is_none() {
            return -1;
        }

        let prot = match MmapProt::from_bits(prot) {
            Some(v) => v,
            None => return -1,
        };
        let flags = match MmapFlags::from_bits(flags) {
            Some(v) => v,
            None => return -1,
        };

        let is_private = flags.contains(MmapFlags::PRIVATE);
        let is_shared = flags.contains(MmapFlags::SHARED);
        if !is_private && !is_shared {
            return -1;
        }
        if is_private && is_shared {
            return -1;
        }

        // Support Anonymous and file-backed mmap
        if flags.contains(MmapFlags::ANONYMOUS) {
            if fd != -1 {
                return -1;
            }
        } else {
            // file-backed
            if fd < 0 {
                return -1;
            }
            if offset % PAGE_SIZE != 0 {
                return -1;
            }
    
            let _backing = match &fd_backing {
                Some(v) => v.clone(),
                _ => return -1,
            };
        }

        let map_len = match len.checked_add(PAGE_SIZE - 1) {
            Some(v) => v & !(PAGE_SIZE - 1),
            None => return -1,
        };
        let is_fixed = flags.contains(MmapFlags::FIXED);

        // Keep a gap below TRAP_CONTEXT.
        let upper = TRAP_CONTEXT_ADDR.saturating_sub(PAGE_SIZE);

        let map_start: usize;
        if is_fixed {
            // MAP_FIXED: force address, and on overlap we must unmap then map.
            if addr.0 % PAGE_SIZE != 0 {
                return -1;
            }
            if addr.0.checked_add(map_len).is_none() {
                return -1;
            }
            if addr.0.saturating_add(map_len) > upper {
                return -1;
            }
            map_start = addr.0;
            if !self.range_is_free(map_start, map_len) {
                if self.unmap_range(VirAddr(map_start), map_len) != 0 {
                    return -1;
                }
            }
        } else {
            // Non-MAP_FIXED: addr is only a hint.
            if addr.0 != 0 {
                let hint: usize = VirAddr(addr.0).floor_down().0 * PAGE_SIZE;
                if hint.saturating_add(map_len) <= upper && self.range_is_free(hint, map_len) {
                    map_start = hint;
                } else {
                    map_start = match self.find_free_range(map_len) {
                        Some(v) => v,
                        None => return -1,
                    };
                }
            } else {
                map_start = match self.find_free_range(map_len) {
                    Some(v) => v,
                    None => return -1,
                };
            }
        }

        let start_vpn: VirNumber = VirAddr(map_start).floor_down();
        let end_vpn: VirNumber = VirAddr(map_start.saturating_add(map_len).saturating_sub(1)).floor_down();
        let range: VirNumRange = VirNumRange(start_vpn, end_vpn);

        let mut mapflags = MapAreaFlags::U;
        if prot.contains(MmapProt::READ) {
            mapflags |= MapAreaFlags::R;
        }
        if prot.contains(MmapProt::WRITE) {
            if fd_backing.is_some() {
                if let Some(file) = &fd_backing{
                    // TODO 权限校验
                }
            }
            mapflags |= MapAreaFlags::W;
        }
        if prot.contains(MmapProt::EXEC) {
            mapflags |= MapAreaFlags::X;
        }
        
        let id = alloc_mmap_id();
       

        let info = MmapInfo {
            id: id,
            flags,
            prot,
            backing: if flags.contains(MmapFlags::ANONYMOUS) {
                None
            } else {
                match fd_backing {
                    Some(v) => Some(v),
                    _ => return -1,
                }
            },
            offset,
        };


        self.add_area(range, MapType::Maped, mapflags, None, Some(info));
        
        map_start as isize
    }

    ///unmap系统调用,取消映射一个[start,end]范围的虚拟页面，并且设置对应页表项不合法
    /// startVAR mmap起始地址 size:映射长度(会被裁剪，小于一个页取消映射一个页,不满一个页补全一个页) 返回-1代表失败 0代表成功 
    pub fn unmap_range(&mut self,startVAR:VirAddr,size:usize,)->isize{
        if size == 0 {
            return -1;
        }
        let start_vpn: VirNumber = startVAR.floor_down();
        let end_vpn: VirNumber = VirAddr(startVAR.0.saturating_add(size).saturating_sub(1)).floor_down();
        let range: VirNumRange = VirNumRange(start_vpn, end_vpn);

        if !self.AallArea_Iscontain_thisVpn_plus(range) {
            return -1;
        }

        let touches_non_mmap = self.areas.iter().any(|area| {
            area.mmap.is_none() && !area.range.is_contain_thisvpnRange(range).is_empty()
        });
        if touches_non_mmap {
            return -1;
        }

        let mut new_areas: Vec<MapArea> = Vec::with_capacity(self.areas.len());
        let mut any_touched = false; //是否有交集

        for area in self.areas.drain(..) {
            let inter = area.range.is_contain_thisvpnRange(range);
            if inter.is_empty() {
                //没有交集
                new_areas.push(area);
                continue;
            }
            any_touched = true;

            let info = match area.mmap.as_ref() {
                Some(v) => v.clone(),
                None => { // 无mmap信息的area,不动
                    new_areas.push(area);
                    continue;
                }
            };

            let area_start = area.range.0;

            let unmap_start = if range.0.0 > area.range.0.0 { range.0 } else { area.range.0 };
            let unmap_end = if range.1.0 < area.range.1.0 { range.1 } else { area.range.1 };
            debug!("Area Before split:{:?} \n",area.range);
            // Split the area to three aread.
            let mut split_aread_vec = Self::split_area_by_range(area, VirNumRange(unmap_start, unmap_end));
            split_aread_vec.0.drain(..).for_each(|x|{
                debug!("After split not need munmap:{:?} \n",x.range);
                new_areas.push(x);
            });
            let mut need_munmap = split_aread_vec.1;

            if info.flags.contains(MmapFlags::SHARED) && info.flags.contains(MmapFlags::ANONYMOUS) {
                let mut shared = SHARED_MMAP_PAGES.lock();
                for vpn in VirNumRange(unmap_start, unmap_end) {
                    let page_index = vpn.0.saturating_sub(need_munmap.range.0.0) as u64;
                    let key = SharedMmapKey::Anon { mmap_id: info.id, page_index };
                    shared.remove(&key);
                }
            }

            //处理有fd情况
            if  info.flags.contains(MmapFlags::SHARED) && !info.flags.contains(MmapFlags::ANONYMOUS){
                let file_backing = match info.backing.as_ref() {
                    Some(v) => v.clone(),
                    None => {
                        panic!("This area should be have a file backend, please check mmap.Will panic every mmap will break file!");
                    }
                };
                for vpn in VirNumRange(unmap_start, unmap_end) {
                    // Only write back pages that were actually faulted-in.
                    let Some(frame) = need_munmap.frames.get(&vpn).cloned() else {
                        continue;
                    };

                    // Keep consistent with pagefault shared key:
                    // file_page = (offset / PAGE_SIZE) + (vpn - area_start)
                    let page_index = vpn.0.saturating_sub(area_start.0) as u64;
                    let file_page = (info.offset / PAGE_SIZE) as u64 + page_index;
                    let inode_num = match file_backing.stat() {
                        Ok(st) => st.inode,
                        Err(_e) => {
                            error!("munmap: MAP_SHARED stat failed kill");
                            TASK_MANAER.kail_current_task_and_run_next();
                            return -1;
                        }
                    };
                    let key = SharedMmapKey::File { inode_num, file_page };

                    // Write the whole page back.
                    let file_off = info.offset.saturating_add((page_index as usize).saturating_mul(PAGE_SIZE));
                    let pa: PhysiAddr = frame.ppn.into();
                    let buf = unsafe { core::slice::from_raw_parts(pa.0 as *const u8, PAGE_SIZE) };
                    match file_backing.write_at(file_off, buf) {
                        Ok(_) => {}
                        Err(e) => {
                            error!("munmap: MAP_SHARED write_back failed off={} err={} kill", file_off, e);
                            TASK_MANAER.kail_current_task_and_run_next();
                            return -1;
                        }
                    }

                    // Remove cache entry for this page.
                    let mut shared = SHARED_MMAP_PAGES.lock();
                    shared.remove(&key);
                }
            }


            for vpn in VirNumRange(unmap_start, unmap_end) {
                if let Some(pte) = self.table.find_pte_vpn(vpn) {
                    pte.set_inValid();
                }
            }

            for vpn in VirNumRange(unmap_start, unmap_end) {
                let _ = need_munmap.frames.remove(&vpn);
            }
            // Fully covered: drop the area.
            continue;
        }

        self.areas = new_areas;
        if any_touched { 0 } else { -1 }
    }


    /// 分割一个area成二/三个不同area noneed,need
    pub fn split_area_by_range(area:MapArea,mid_range:VirNumRange)->(Vec<MapArea>,MapArea){
        debug!("Will be munmap:{:?} \n",mid_range);
        let mut area =area;
        if mid_range.0.0 <= area.range.0.0 && mid_range.1.0 >= area.range.1.0{
            let re:Vec<MapArea>=Vec::new();
            return (re,area);
        }
        // 首先收集不需要munmap的frame
        //需要包含
        
        let start_vpn = if mid_range.0.0 >= area.range.0.0 {mid_range.0.0} else {area.range.0.0}; 
        let end_vpn = if mid_range.1.0 >= area.range.1.0 { area.range.1.0} else {mid_range.1.0};

        if start_vpn > area.range.0.0 && end_vpn < area.range.1.0{
            let left_noneed_frametrace = area.frames.split_off(&VirNumber(start_vpn));
            let right_noneed_frametrace = area.frames.split_off(&VirNumber(end_vpn+1));
            let need_frametrace = area.frames;
            let mut re:Vec<MapArea>=Vec::new();
            let mut left = MapArea::new(VirNumRange(area.range.0,VirNumber(start_vpn-1) ), area.flags, area.map_type);
            left.frames = left_noneed_frametrace;
            left.mmap = area.mmap.clone();
            let mut mid = MapArea::new(mid_range, area.flags, area.map_type);
            mid.frames = need_frametrace;
            mid.mmap = area.mmap.clone();
            let mut right = MapArea::new(VirNumRange(VirNumber(end_vpn+1),area.range.1), area.flags, area.map_type);
            right.frames = right_noneed_frametrace;
            right.mmap = area.mmap.clone();
            re.push(left);
            re.push(right);
            return (re,mid);
        }else {
            let mut life_range:VirNumRange = VirNumRange(VirNumber(0), VirNumber(0));
            if start_vpn> area.range.0.0{life_range=VirNumRange(area.range.0, VirNumber(start_vpn-1))}else {life_range=VirNumRange(VirNumber(end_vpn+1), area.range.1)}
            let mut re:Vec<MapArea>=Vec::new();
            let mut no_munmap:BTreeMap<VirNumber,Arc<FramTracker>>=BTreeMap::new();
            let mut need_munmap:BTreeMap<VirNumber,Arc<FramTracker>>=BTreeMap::new();
            area.frames.into_iter().for_each(|kv|{
                if !mid_range.is_contain_thisvpn(kv.0){
                    //不需要处理的
                    no_munmap.insert(kv.0, kv.1);
                }else {
                    //需要处理munmap的
                    need_munmap.insert(kv.0, kv.1);
                }
            });
            let mut no_new_area = MapArea::new(life_range, area.flags, area.map_type);
            no_new_area.frames=no_munmap;
            no_new_area.mmap=area.mmap.clone();
            let mut need_new_area = MapArea::new(mid_range, area.flags, area.map_type);
            need_new_area.frames = need_munmap;
            need_new_area.mmap=area.mmap.clone();
            re.push(no_new_area);
            return (re,need_new_area);
        }

        
    }



    ///从elf解析数据创建应用地址空间 Mapset entry user_stack,kernel_sp
    /// elf_data: ELF 文件数据（可以从文件系统读取）
    pub fn from_elf(elf_data:&[u8])->Option<(Self,usize,VirAddr,usize)>{ 
        let mut memory_set = Self::new_bare();
        // map program headers of elf, with U flag
        let re = xmas_elf::ElfFile::new(elf_data);
        if re.is_err(){
            warn!("Can't parsing this raw data to elf");
            return None;
        }
        let elf = re.expect("Kernel error");
        let elf_header = elf.header;
        let magic = elf_header.pt1.magic;
        if magic != [0x7f, 0x45, 0x4c, 0x46]{
            warn!("not a elf file");
            return None;
        }
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

                memory_set.add_area(
                    VirNumRange::new(start_va, end_va), // bug!!!!!! 范围range权限覆盖
                    MapType::Maped,
                    map_perm,
                    Some((start_va.0%PAGE_SIZE,&elf.input[ph.offset() as usize..(ph.offset() + ph.file_size()) as usize])),
                    None
                );//应用area默认default
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
        memory_set.add_area(
            VirNumRange(userstack_start_vpn,userstack_end_vpn),
            MapType::Maped,
            MapAreaFlags::W | MapAreaFlags::R | MapAreaFlags::U,
            None,
            None,
            
        );
        //映射用户堆 初始0 通过brk生长---------------------------------------------+0
        let userheap_start_end_vpn = VirNumber(userstack_end_vpn.0+1);//无需guardpage，堆不会向下溢出
        debug!("  Mapping user heap: vpn={:#x}", userheap_start_end_vpn.0);
        memory_set.add_area(
            VirNumRange(userheap_start_end_vpn, userheap_start_end_vpn),
            MapType::Maped,
            MapAreaFlags::R | MapAreaFlags::W | MapAreaFlags::U,
            None,
            None,
        );

        //设置brk
        memory_set.brk = userheap_start_end_vpn.into();

        // 分配并映射内核栈（使用全局分配器，不再依赖 appid）
        let kernel_stack_top = Self::alloc_kernel_stack();


        //打印权限信息
        //memory_set.print_area_information();

        Some(
        (
            memory_set,
            entry_point as usize,
            user_sp,
            kernel_stack_top
        )
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
            None,
        );

        kernel_stack_top
    }


    pub(crate) fn new_bare()->Self{
        MapSet{
            table:PageTable::new(),
            areas:Vec::new(),
            brk:VirAddr(0)
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
            MapType::Maped,
            MapAreaFlags::R | MapAreaFlags::W,
            None,
            None,
        );
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
            !area.range.is_contain_thisvpnRange(vpnrange).is_empty()
        })
    }

    ///获取所有包含范围内vpn的maparea的实体move所有权
    pub fn pop_contain_range_area(&mut self,range:VirNumRange)->Vec<MapArea>{
        let mut result:Vec<MapArea>=Vec::new();//存放结果 
        let index:Vec<usize> = self.areas.iter().enumerate().filter(|(_,area)|{
            !area.range.is_contain_thisvpnRange(range).is_empty()
        }).map(|(index,_)|{index}).collect();
        for inde in index{
           result.push(self.areas.remove(inde));
        }
        result
        
    }

    ///输入range，maptype和flags 自动处理maparea的映射和物理帧挂载以及对应memset的pagetable映射,处理数据的复制映射   但是映射用户栈不需要数据
    pub fn add_area(&mut self,range:VirNumRange,map_type :MapType,flags:MapAreaFlags,data:Option<(usize,&[u8])>,mmap:Option<MmapInfo>){
        let mut area=MapArea::new(range, flags, map_type);
        area.mmap = mmap;
        if area.mmap.is_none() {

            //查找已经vpn映射过的ppn 以range.0为目标
            let target = range.left_point();

            let mut find_re:Option<Arc<FramTracker>> = None;

            find_re=self.find_thisvpn_frame(target);


            // 一个area只能有一个重叠目标
            area.map_all(&mut self.table,find_re);//映射area,处理物理页帧分配逻辑
            if let MapType::Maped = map_type{//maped方式要复制数据
                area.copy_data(data, &mut self.table);
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
        mem_set.add_area(
            hardware_range,
            MapType::Indentical,
            MapAreaFlags::R | MapAreaFlags::W,
            None,
            None,
        );

        //映射代码段
        let text_range = VirNumRange::new(VirAddr(stext as usize), VirAddr(etext as usize));//range封装过
        mem_set.add_area(
            text_range,
            MapType::Indentical,
            MapAreaFlags::R | MapAreaFlags::X,
            None,
            None,
        );


        //映射rodata段
        let rodata_range = VirNumRange::new(VirAddr(srodata as usize), VirAddr(erodata as usize));//range封装过
        mem_set.add_area(
            rodata_range,
            MapType::Indentical,
            MapAreaFlags::R,
            None,
            None,
        );
        //trace!("{} {}\n",rodata_start_vpn.0,rodata_end_vpn.0);

    
        // 映射内核数据段
        let data_range = VirNumRange::new(VirAddr(sdata as usize), VirAddr(edata as usize));//range封装过
        mem_set.add_area(
            data_range,
            MapType::Indentical,
            MapAreaFlags::R | MapAreaFlags::W,
            None,
            None,
        );
       // trace!("{} {}\n",data_start.0,data_end.0);

        //映射bss段
        let bss_range = VirNumRange::new(VirAddr(sbss as usize), VirAddr(ebss as usize));//range封装过
        mem_set.add_area(
            bss_range,
            MapType::Indentical,
            MapAreaFlags::R | MapAreaFlags::W,
            None,
            None,
        );
       // trace!("{} {}\n",bss_start.0,bss_end.0);
        
        // 映射物理内存(必须手动构造range区间)，phystart需要向上取整,end需要手动-1 range
        let phys_start =VirAddr(ekernel as usize).floor_up();
        let phys_end =VirAddr(ekernel as usize + MEMORY_SIZE-PAGE_SIZE).floor_down(); //ekernel 为结束地址 end需要手动-1 range
        let phys_range = VirNumRange(phys_start,phys_end);
        mem_set.add_area(
            phys_range,
            MapType::Indentical,
            MapAreaFlags::W | MapAreaFlags::R,
            None,
            None,
        );
       // trace!("{} {}\n",phys_start.0,phys_end.0);

        //设置brk
        mem_set.brk = VirAddr(ebss as usize + 1);

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

impl Drop for MapSet {
    fn drop(&mut self) {
        for area in self.areas.iter() {
            let Some(info) = area.mmap.as_ref() else {
                continue;
            };


            if info.flags.contains(MmapFlags::SHARED) && info.flags.contains(MmapFlags::ANONYMOUS) {
                let mut shared = SHARED_MMAP_PAGES.lock();
                for vpn in VirNumRange(area.range.0, area.range.1) {
                    let page_index = vpn.0.saturating_sub(area.range.0.0) as u64;
                    let key = SharedMmapKey::Anon { mmap_id: info.id, page_index };
                    shared.remove(&key);
                }
                continue;
            }

            if info.flags.contains(MmapFlags::SHARED) && !info.flags.contains(MmapFlags::ANONYMOUS) {
                let file_backing = match info.backing.as_ref() {
                    Some(v) => v.clone(),
                    None => continue,
                };

                for (vpn, frame) in area.frames.iter() {
                    
                    let page_index = vpn.0.saturating_sub(area.range.0.0) as u64;
                    let file_page = (info.offset / PAGE_SIZE) as u64 + page_index;
                    let inode_num = match file_backing.stat() {
                        Ok(st) => st.inode,
                        Err(_e) => {
                            // On drop path, avoid killing task; best-effort cleanup.
                            continue;
                        }
                    };
                    let key = SharedMmapKey::File { inode_num, file_page };
                    let file_off = info.offset.saturating_add((page_index as usize).saturating_mul(PAGE_SIZE));
                    let pa: PhysiAddr = frame.ppn.into();
                    let buf = unsafe { core::slice::from_raw_parts(pa.0 as *const u8, PAGE_SIZE) };
                    let _ = file_backing.write_at(file_off, buf);
                    let mut shared = SHARED_MMAP_PAGES.lock();
                    shared.remove(&key);
                }
            }
        }
    }
}