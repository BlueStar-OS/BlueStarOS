use core::arch::global_asm;
use core::panicking::panic;

use alloc::collections::vec_deque::VecDeque;
use alloc::string::String;
use alloc::string::ToString;
use alloc::sync::Arc;
use alloc::sync::Weak;
use alloc::vec;
use alloc::vec::Vec;
use lazy_static::lazy_static;
use log::error;
use log::trace;
use log::warn;
use riscv::register::sstatus;
use riscv::register::sstatus::SPP;
use rsext4::OpenFile;
use crate::__kernel_refume;
use crate::config::*;
use crate::fs::vfs::FileDescriptor;
use crate::fs::vfs::OpenFlags;
use crate::memory::*;
use crate::sbi::shutdown;
use crate::task::file_loader;
use log::debug;
use crate::fs::component::stdio::stdio::{stdin_fd, stdout_fd, stderr_fd};
use crate::trap::{app_entry_point, kernel_trap_handler};
///init进程PID
pub const INIT_PID:usize=1;

///任务上下文
use crate::{ sync::UPSafeCell, trap::TrapContext};
global_asm!(include_str!("_switch.S"));

#[repr(C)]
#[derive(Clone)]
pub struct TaskContext{
     ra:usize, //offset 0
     pub sp:usize, //offser 8
     ///s0-s11 被调用者保存寄存器 switch保存
     calleed_register:[usize;12]//offset 16-..
}

#[derive(Clone)]
pub enum TaskStatus {
    //UnInit,
    Runing,
    Zombie,
   // Blocking,
    Ready,
}


///进程id 需要实现回收 rail自动分配
#[derive(Clone)]
pub struct  ProcessId(pub usize);

///进程id分配器 需要实现分配 [start,end)
pub struct ProcessIdAlloctor{
    current:usize,//当前的pid
    end:usize,//最高限制的pid，可选
    id_pool:Vec<ProcessId>
}

#[derive(Clone)]
pub struct TaskControlBlock{
        pub pid:ProcessId,                              //进程id
        pub memory_set:MapSet,                          //程序地址空间
        pub task_statut:TaskStatus,                         //程序运行状态
        pub exit_code:isize,
        pub task_context:TaskContext,                       //任务上下文
        pub trap_context_ppn:usize,                         //陷阱上下文物理帧
        pass:usize,                                     //行程
        stride:usize,                                   //步长
        ticket:usize,                                   //权重
        pub file_descriptor:Vec<Option<Arc<FileDescriptor>>>,       //文件描述符表
        cwd:String,         //进程工作的路径 默认/
        pub parent:Option<Weak<UPSafeCell<TaskControlBlock>>>,                  //父进程弱引用
        pub childrens:Vec<Arc<UPSafeCell<TaskControlBlock>>>            //子进程强引用
}








pub struct TaskManagerInner{
    pub task_queen:VecDeque<Arc<UPSafeCell<TaskControlBlock>>>,//任务队列
    pub current:usize//当前任务
}

///任务管理器
pub struct TaskManager{//单核环境目前无竞争
    ///注意释放时机
   pub task_que_inner:UPSafeCell<TaskManagerInner>,//内部可变性 
}

impl  ProcessIdAlloctor{
    ///初始化进程id分配器 start:起始分配pid end:限制最大的pid
    pub fn initial_processid_alloctor(start:usize,end:usize)->Self{
        let id_pool :Vec<ProcessId>= Vec::new();
            ProcessIdAlloctor { current: start, end ,id_pool:id_pool}
    }

    ///分配进程id
    pub fn alloc_id(&mut self)->Option<ProcessId>{
        //首先检查pool是否有可用process
        if !self.id_pool.is_empty(){
          return self.id_pool.pop();
        }
        //检查边界 ，先把currentid+1，然后返回
        if self.current < self.end{
            self.current+=1;
           return Some(ProcessId(self.current-1));
        }

        None
    }
}


impl Drop for ProcessId {
    fn drop(&mut self) {
        ///进程id自动回收 rail思想 需要先初始化全局processidalloctor
        ProcessId_ALLOCTOR.lock().id_pool.push(ProcessId(self.0));//实际只需要保存id号
        trace!("Process Id :{} recycled!",self.0)
    }
}


impl TaskContext {
    /// 创建任务上下文，跳转到 app_entry_point
    /// 注意：kernel_sp 是内核栈指针，不是用户栈！
    /// app_entry_point 是内核函数，需要内核栈来执行
    pub fn return_trap_new(kernel_sp: usize) -> Self {
       TaskContext { ra: app_entry_point as usize, sp: kernel_sp, calleed_register: [0;12] }
    }
///零初始化
    pub fn zero_init()->Self{
        TaskContext { ra: 0, sp: 0, calleed_register: [0;12] }
    }
}

impl TaskControlBlock {

    fn align_up(x: usize, align: usize) -> usize {
        (x + align - 1) & !(align - 1)
    }

    fn push_args_to_user_stack(satp: usize, user_sp: usize, argv: &[String]) -> usize {
        let argc = argv.len();
        let mut total = core::mem::size_of::<usize>();
        for a in argv.iter() {
            total += Self::align_up(a.as_bytes().len() + 1, 8);
        }
        total = Self::align_up(total, 8);

        let new_sp = user_sp.saturating_sub(total) & !7usize;
        let mut blob: alloc::vec::Vec<u8> = alloc::vec::Vec::with_capacity(total);
        blob.extend_from_slice(&argc.to_ne_bytes());
        for a in argv.iter() {
            blob.extend_from_slice(a.as_bytes());
            blob.push(0);
            while blob.len() % 8 != 0 {
                blob.push(0);
            }
        }
        while blob.len() < total {
            blob.push(0);
        }

        let mut slices = PageTable::get_mut_slice_from_satp(satp, blob.len(), VirAddr(new_sp));
        let mut off = 0usize;
        for s in slices.iter_mut() {
            if off >= blob.len() {
                break;
            }
            let n = core::cmp::min(s.len(), blob.len() - off);
            s[..n].copy_from_slice(&blob[off..off + n]);
            off += n;
        }
        new_sp
    }

    ///设置父亲进程引用
    pub fn set_father(&mut self,father:&Arc<UPSafeCell<TaskControlBlock>>){
        self.parent = Some(Arc::downgrade(&father));
    }

    ///添加子进程引用
    pub fn add_children(&mut self,tlb:Arc<UPSafeCell<TaskControlBlock>>){
        self.childrens.push(tlb);
    }

    pub fn get_cwd(&self) -> &str {
        &self.cwd
    }

    pub fn set_cwd(&mut self, cwd: String) {
        self.cwd = cwd;
    }

    ///exec换血
    /// path:可执行文件位置
    /// 不应该返回
    pub fn new_exec_task(&mut self,path:&str,argv:Vec<String>,argc:usize) -> bool {
        //按照new函数来换血，换内核栈，换地址空间
        debug!("exec: replacing current task image with {}  <----ptah\n", path);
    
        let elf_data = file_loader(path);
        if elf_data.is_empty() {
            //加载错误,直接返回
            return false;
        }
        debug!("Load success");
        let (mut memset, elf_entry, user_sp, kernel_sp) = MapSet::from_elf(&elf_data);
        let task_cx = TaskContext::return_trap_new(kernel_sp);
        let kernel_satp = KERNEL_SPACE.lock().table.satp_token();
        let user_satp = memset.table.satp_token();
        let trap_cx_ppn = memset
            .table
            .translate_byvpn(VirAddr(TRAP_CONTEXT_ADDR).strict_into_virnum())
            .expect("trap ppn translate failed");

        //把命令行参数推入用户栈
        let _ = argc;
        let new_user_sp = Self::push_args_to_user_stack(user_satp, user_sp.0, &argv);



        self.memory_set = memset;//释放旧的全复制地址空间
   
        self.task_context = task_cx;
        self.trap_context_ppn = trap_cx_ppn.0;

        let trap_cx_point: *mut TrapContext = (trap_cx_ppn.0 * PAGE_SIZE) as *mut TrapContext;
        unsafe {
            *trap_cx_point = TrapContext::init_app_trap_context(
                elf_entry,
                kernel_satp,
                kernel_trap_handler as usize,
                kernel_sp, //此时被换血进程的内核栈指针还是用的旧的，只要不立马扫清旧内核栈就没事的
                new_user_sp,
            );
        }
        true
    }
    

    /// 创建新任务
    fn new(app_path: &str, _kernel_stack_id: usize,father:Option<Weak<UPSafeCell<TaskControlBlock>>>) -> Self {
        debug!("Creating task for app_path: {}, kernel_stack_id: {}", app_path, _kernel_stack_id);
        
        let elf_data = file_loader(app_path);
        let (mut memset, elf_entry, user_sp, kernel_sp) = MapSet::from_elf(&elf_data);
        let task_cx = TaskContext::return_trap_new(kernel_sp);
        let kernel_satp = KERNEL_SPACE.lock().table.satp_token();
        let user_satp = memset.table.satp_token();
        let trap_cx_ppn = memset.table
            .translate_byvpn(VirAddr(TRAP_CONTEXT_ADDR).strict_into_virnum())
            .expect("trap ppn translate failed");

        let argv = alloc::vec![alloc::string::String::from(app_path)];
        let new_user_sp = Self::push_args_to_user_stack(user_satp, user_sp.0, &argv);
        
        // 初始化文件描述符表：0=stdin, 1=stdout, 2=stderr
        let mut file_descriptor_table: Vec<Option<Arc<FileDescriptor>>> = Vec::new();
        file_descriptor_table.push(Some(stdin_fd()));
        file_descriptor_table.push(Some(stdout_fd()));
        file_descriptor_table.push(Some(stderr_fd()));
        
        let task_control_block = TaskControlBlock {
            pid:ProcessId_ALLOCTOR.lock().alloc_id().expect("No Process ID Can use"),
            memory_set: memset,
            task_statut: TaskStatus::Ready,
            exit_code: 0,
            task_context: task_cx,
            trap_context_ppn: trap_cx_ppn.0,
            pass: 0,
            stride: BIG_INT / TASK_TICKET,
            ticket: TASK_TICKET,
            file_descriptor: file_descriptor_table,
            cwd:"/".to_string(),
            parent:father,
            childrens:Vec::new()
        };
        
        // 初始化 TrapContext
        let trap_cx_point: *mut TrapContext = (trap_cx_ppn.0 * PAGE_SIZE) as *mut TrapContext;
        unsafe {
            *trap_cx_point = TrapContext::init_app_trap_context(
                elf_entry,
                kernel_satp,
                kernel_trap_handler as usize,
                kernel_sp,
                new_user_sp
            );
        }
        
        debug!("Task created successfully: entry={:#x}, user_sp={:#x}", elf_entry, user_sp.0);
        task_control_block
    }
}

impl Drop for TaskControlBlock {
    fn drop(&mut self) {
        for child in self.childrens.iter() {
            let mut c = child.lock();
            c.parent = None;
        }
        self.childrens.clear();
    }
}


impl TaskManager {//全局唯一

    pub fn mark_current_zombie(&self, exit_code: isize) {
        let inner = self.task_que_inner.lock();
        if inner.task_queen.is_empty() {
            drop(inner);
            panic!("Task Queen is empty!");
        }
        let current = inner.current;
        if current >= inner.task_queen.len() {
            drop(inner);
            panic!("TaskManager current index out of range");
        }
        {
            let mut t = inner.task_queen[current].lock();
            t.task_statut = TaskStatus::Zombie;
            t.exit_code = exit_code;
        }
        drop(inner);
    }

    pub fn reparent_current_children_to_init(&self) {
        let inner = self.task_que_inner.lock();
        if inner.task_queen.is_empty() {
            drop(inner);
            return;
        }
        let current = inner.current;
        if current >= inner.task_queen.len() {
            drop(inner);
            return;
        }
        let current_task = inner.task_queen[current].clone();
        let init_task = inner
            .task_queen
            .iter()
            .find(|t| t.lock().pid.0 == INIT_PID)
            .cloned();
        drop(inner);

        let Some(init_task) = init_task else {
            return;
        };
        if current_task.lock().pid.0 == INIT_PID {
            return;
        }
        let children = {
            let mut parent = current_task.lock();
            core::mem::take(&mut parent.childrens)
        };
        if children.is_empty() {
            return;
        }
        let init_weak = Arc::downgrade(&init_task);
        {
            let mut init = init_task.lock();
            for child in children.iter() {
                init.childrens.push(child.clone());
            }
        }
        for child in children {
            let mut c = child.lock();
            c.parent = Some(init_weak.clone());
        }
    }

    pub fn reap_zombie_child(&self, child_pid: usize) -> Option<isize> {
        let mut inner = self.task_que_inner.lock();
        let current = inner.current;
        if inner.task_queen.is_empty() || current >= inner.task_queen.len() {
            drop(inner);
            return None;
        }
        let current_task = inner.task_queen[current].clone();

        // 只允许回收当前进程的子进程，防止误回收其他任务的 Zombie。
        let is_child_of_current = {
            let parent = current_task.lock();
            parent.childrens.iter().any(|c| c.lock().pid.0 == child_pid)
        };
        if !is_child_of_current {
            drop(inner);
            return None;
        }

        let mut zombie_index: Option<usize> = None;
        let mut exit_code: Option<isize> = None;
        for (idx, cell) in inner.task_queen.iter().enumerate() {
            let t = cell.lock();
            if t.pid.0 == child_pid {
                if matches!(t.task_statut, TaskStatus::Zombie) {
                    zombie_index = Some(idx);
                    exit_code = Some(t.exit_code);
                }
                break;
            }
        }
        let Some(idx) = zombie_index else {
            drop(inner);
            return None;
        };

        inner.task_queen.remove(idx);
        if !inner.task_queen.is_empty() {
            if inner.current > idx {
                inner.current -= 1;
            } else if inner.current >= inner.task_queen.len() {
                inner.current = 0;
            }
        }
        drop(inner);

        {
            let mut parent = current_task.lock();
            parent.childrens.retain(|c| c.lock().pid.0 != child_pid);
        }
        exit_code
    }

    ///TODO:根据传入路径加载并且new新的taskblock然后add_task进队列
    pub fn load_newtask_to_taskmanager(path:&str){

    }

    ///添加任务队列或者归队
    pub fn add_task(self,task:Arc<UPSafeCell<TaskControlBlock>>){
        self.task_que_inner.lock().task_queen.push_back(task);
    }

    ///根据stride选择一个任务 (index, pass)
    ///
    ///注意：这是一个对外包装，会持锁一次。若调用方已经持有 inner 锁，必须使用
    ///`stride_select_task_inner`，否则会触发 `RefCell already borrowed`。
    pub fn stride_select_task(&self)->Option<(usize, usize)>{
        let inner  = self.task_que_inner.lock();
        Self::stride_select_task_inner(&inner)
    }

    ///在已持有 TaskManagerInner 锁的情况下选择任务（不会再次 lock）。
    fn stride_select_task_inner(inner: &TaskManagerInner) -> Option<(usize, usize)> {
        let current = inner.current;
        let mut selected: Option<(usize, usize)> = None; // (index, pass)
        for (idx, cell) in inner.task_queen.iter().enumerate() {
            let t = cell.lock();
            if let TaskStatus::Ready = t.task_statut {
                let pass = t.pass;
                match selected {
                    Some((best_idx, best_pass)) => {
                        if pass < best_pass {
                            selected = Some((idx, pass));
                        } else if pass == best_pass {
                            if best_idx == current && idx != current {
                                selected = Some((idx, pass));
                            }
                        }
                    }
                    None => selected = Some((idx, pass)),
                }
            }
        }
        selected
    }

    ///TODO 内核栈释放
    ///从队列移除当前任务,应该由aplication的exit系统调用来执行 之后必须执行下一个任务 bug修复：应该同时移动指针到任意一个ready的任务
    pub fn remove_current_task(&self){
        let mut inner=self.task_que_inner.lock();
        
        // 先保存要删除的任务索引
        let task_to_remove = inner.current;
        debug!("Removing task at index: {}, queue length before removal: {}", task_to_remove, inner.task_queen.len());

        let removed_task = inner.task_queen[task_to_remove].clone();
        let init_task = inner
            .task_queen
            .iter()
            .find(|t| t.lock().pid.0 == INIT_PID)
            .cloned();
        
        // 删除任务
        inner.task_queen.remove(task_to_remove).expect("Remove Task Control Block Failed!");
        
        // 删除后更新current指针
        // VecDeque.remove(i) 会删除索引i的元素，后面的元素索引都会减1
        // 删除后，如果还有任务，我们需要将current设置为一个有效的任务索引
        if !inner.task_queen.is_empty() {
            // 如果删除的是最后一个任务（task_to_remove == 原队列长度-1）
            // 则删除后 task_to_remove >= 新队列长度，需要回绕到开头
            if task_to_remove > inner.task_queen.len() {
                panic!("Task Remove Faild , kernel try to remove a task that index over taskqueen")
            }
            if task_to_remove == inner.task_queen.len() {
                //并且选择一个已经ready的任务，防止执行流错误。
                let select_task = Self::stride_select_task_inner(&inner);
                if select_task.is_none(){
                    error!("After remove,No task can select");
                    shutdown();
                }else {
                    inner.current = select_task.unwrap().0; //current
                }
                
            } else {
                // 否则，保持current在原位置
                // 此时current指向的是原来task_to_remove+1位置的任务
                inner.current = task_to_remove; //防御延迟到调度函数
            }
            debug!("After removal: current set to {}, queue length: {}", inner.current, inner.task_queen.len());
        }
        
        drop(inner);

        if let Some(init_task) = init_task {
            if removed_task.lock().pid.0 != INIT_PID {
                let children = {
                    let mut parent = removed_task.lock();
                    core::mem::take(&mut parent.childrens)
                };
                if !children.is_empty() {
                    let init_weak = Arc::downgrade(&init_task);
                    {
                        let mut init = init_task.lock();
                        for child in children.iter() {
                            init.childrens.push(child.clone());
                        }
                    }
                    for child in children {
                        let mut c = child.lock();
                        c.parent = Some(init_weak.clone());
                    }
                }
            }
        } else {
            error!("Can't find init process!,these child process will become foster process");
            let children = {
                let mut parent = removed_task.lock();
                core::mem::take(&mut parent.childrens)
            };
            for child in children {
                let mut c = child.lock();
                c.parent = None;
            }
        }

        if self.task_queen_is_empty() {
            error!("The last task(should be init) exit or be removed,shutdown");
            shutdown();
        }
    }
    ///根据Stride挑选下个要运行的READY任务,挂起当前任务,把current设置为下个任务的index,然后运行下一个任务 Stride算法：增加运行任务的步长
    pub fn suspend_and_run_task(&self){ //首先应该检查任务是否为空

        //任务列表是否为空?
        if self.task_queen_is_empty(){
                panic!("Task Queen is empty!");
        }


        let mut inner  =self.task_que_inner.lock();
        if inner.task_queen.is_empty() {
            drop(inner);
            panic!("Task Queen is empty!");
        }
        let current = inner.current;
        if current >= inner.task_queen.len() {
            drop(inner);
            panic!("TaskManager current index out of range");
        }

        {
            let mut cur = inner.task_queen[current].lock();
            if !matches!(cur.task_statut, TaskStatus::Zombie) {
                cur.task_statut = TaskStatus::Ready;
                cur.pass += cur.stride;
            }
        }

        // 选择 stride 最小的 READY 任务（注意：这里已持有 inner 锁，不能再次 lock）
        let selected: Option<(usize, usize)> = Self::stride_select_task_inner(&inner);
        let task_index = match selected {
            Some((idx, _)) => idx,
            None => {
                error!("No task can select");
                shutdown();
            }
        };
        if task_index >= inner.task_queen.len() {
            drop(inner);
            panic!("Selected task index out of range");
        }
        {
            let task_status;
            {
                let t = inner.task_queen[task_index].lock();
                task_status= t.task_statut.clone();
            }
            match task_status {
                TaskStatus::Ready => {}
                _ => {
                    drop(inner);
                    panic!("Selected task is not Ready");
                }
            }
        }
        
        debug!("current:{} Next task:{}",inner.current,task_index);
        
        //如果切换到同一个任务，直接返回 _switch耗费上下文资源
        //这可以防止在持有用户态锁时发生任务切换导致的死锁问题（全局锁）
        if current == task_index {
            {
                let mut t = inner.task_queen[task_index].lock();
                t.task_statut = TaskStatus::Runing;
            }
            drop(inner);
            debug!("Same task, skip __switch");
            return;
        }
        
        // 准备切换：先拿到上下文指针，更新状态，然后释放所有锁再 __switch
        let swaped_task_cx = {
            let cur = inner.task_queen[current].lock();
            &cur.task_context as *const TaskContext
        };

        let need_swap_in = {
            let mut next = inner.task_queen[task_index].lock();
            next.task_statut = TaskStatus::Runing;
            next.pass += next.stride;
            &mut next.task_context as *mut TaskContext
        };

        inner.current = task_index;
        drop(inner);
        unsafe {
            __switch(swaped_task_cx, need_swap_in);
        }

        //任务从这里返回
    }

    pub fn task_queen_is_empty(&self)->bool{
        let inner=self.task_que_inner.lock();
        let result= inner.task_queen.is_empty();
        drop(inner);
        debug!("task queen empty?:{}",result);
        result
    }


    ///运行第一个任务
    pub fn run_first_task(&self) -> ! {
      let inner=self.task_que_inner.lock();//记得drop
      let curren_task_index=inner.current;
      let task_cx_ptr = {
        let mut task = inner.task_queen[curren_task_index].lock();
        // 标记为 running
        task.task_statut = TaskStatus::Runing;
        // 增加步长
        task.pass += task.stride;
        &mut task.task_context as *mut TaskContext
      };
      let kernel_task_cx=TaskContext::zero_init();
      drop(inner);//越早越好
      // 调用 __switch 切换到第一个任务
      // __switch 会：
      // 1. 保存 _unused 的上下文（虽然我们不会再用到）
      // 2. 恢复 next_task_cx_ptr 指向的上下文
      // 3. 跳转到 task.task_context.ra，即 app_entry_point
      unsafe {
        __switch(&kernel_task_cx as *const TaskContext, task_cx_ptr);
      }
      
      panic!("unreachable in run_first_task!");
    }

    /// 在删除当前任务后，直接切换到当前 inner.current 指向的任务。
    ///
    /// 关键点：此时 CPU 仍在“已删除任务”的内核栈/执行流上，不能把当前寄存器保存到
    /// 新任务的 `task_context` 里，否则会污染新任务上下文。
    pub fn run_current_task(&self) -> ! {
        let inner = self.task_que_inner.lock();
        if inner.task_queen.is_empty() {
            drop(inner);
            panic!("Task Queen is empty!");
        }
        let current = inner.current;
        if current >= inner.task_queen.len() {
            drop(inner);
            panic!("TaskManager current index out of range");
        }
        let task_cx_ptr = {
            let mut task = inner.task_queen[current].lock();
            task.task_statut = TaskStatus::Runing;
            task.pass += task.stride;
            &mut task.task_context as *mut TaskContext
        };
        let dummy = TaskContext::zero_init();
        drop(inner);
        unsafe {
            __switch(&dummy as *const TaskContext, task_cx_ptr);
        }
        panic!("unreachable in run_current_task!");
    }

    ///获取当前任务的页表stap
    pub fn get_current_stap(&self)->usize{
        let inner= self.task_que_inner.lock();
        let current_task:usize=inner.current;
        let stap = {
            let mut task = inner.task_queen[current_task].lock();
            task.memory_set.get_table().satp_token()
        };
        drop(inner);
        stap
    }

    ///获取当前任务的陷阱上下文可变引用
    pub fn get_current_trapcx(&self)->&mut TrapContext{
        let inner =self.task_que_inner.lock();
        let curren_task_index=inner.current;
        let task_trap_ppn = {
            let task = inner.task_queen[curren_task_index].lock();
            task.trap_context_ppn
        };
        let origin_phyaddr =( task_trap_ppn*PAGE_SIZE) as *mut TrapContext;
        let trap_context =unsafe {
            &mut *origin_phyaddr
        };
        drop(inner);
        trap_context
    }

    ///获取当前任务的文件描述符
    pub fn get_current_fd(&self, fd: usize) -> Option<Option<Arc<FileDescriptor>>> {
        let inner = self.task_que_inner.lock();
        let current_task = inner.current;
        let result = {
            let task = inner.task_queen[current_task].lock();
            task.file_descriptor.get(fd).cloned()
        };
        drop(inner);
        result
    }

    pub fn get_current_cwd(&self) -> String {
        if self.task_que_inner.lock().task_queen.is_empty(){
            return "/".to_string();
        }
        let inner = self.task_que_inner.lock();
        let current_task = inner.current;
        let cwd = {
            let task = inner.task_queen[current_task].lock();
            task.get_cwd().to_string()
        };
        drop(inner);
        cwd
    }

    pub fn set_current_cwd(&self, cwd: String) {
        let inner = self.task_que_inner.lock();
        let current_task = inner.current;
        {
            let mut task = inner.task_queen[current_task].lock();
            task.set_cwd(cwd);
        }
        drop(inner);
    }

    pub fn alloc_fd_for_current(&self, new_fd: Arc<FileDescriptor>) -> i32 {
        let inner = self.task_que_inner.lock();
        let current_task = inner.current;
        let mut task = inner.task_queen[current_task].lock();
        if task.file_descriptor.len() < 2 {
            while task.file_descriptor.len() < 2 {
                task.file_descriptor.push(None);
            }
        }
        for (i, slot) in task.file_descriptor.iter_mut().enumerate() {
            if i < 2 {
                continue;
            }
            if slot.is_none() {
                *slot = Some(new_fd);
                return i as i32;
            }
        }
        task.file_descriptor.push(Some(new_fd));
        (task.file_descriptor.len() - 1) as i32
    }

    pub fn close_current_fd(&self, fd: usize) -> isize {
        let inner = self.task_que_inner.lock();
        let current_task = inner.current;
        let mut task = inner.task_queen[current_task].lock();
        if fd >= task.file_descriptor.len() {
            return -1;
        }
        if task.file_descriptor[fd].is_none() {
            return -1;
        }
        task.file_descriptor[fd] = None;
        0
    }


    ///kail当前任务，内核有权调用 调用栈顶必须为TrapHandler! 调用它的地方考虑是否直接return
    pub fn kail_current_task_and_run_next(&self){
        self.remove_current_task();//删除对应任务块
        self.suspend_and_run_task();//调度下一个stride最小的任务
        error!("Task Kailed!");
    }



}


impl TaskContext {
    ///ra设置为trap refume地址，sp为用户栈指针，callee_register初始化0
    pub fn trapnew_init(sp:usize)->Self{
       TaskContext { ra: __kernel_refume as usize, sp: sp, calleed_register: [0;12] }
    }
}


//全局进程id分配器
lazy_static!{
    pub static ref ProcessId_ALLOCTOR:UPSafeCell<ProcessIdAlloctor>=UPSafeCell::new(ProcessIdAlloctor::initial_processid_alloctor(1, 10_000_000));
}

// 全局任务管理器，加载init程序
lazy_static! {
    pub static ref TASK_MANAER: TaskManager = unsafe {
        debug!("Initializing TASK_MANAGER...");

        let mut task_deque = VecDeque::new();
        
        // 加载init应用程序
            debug!("Loading init lication {}...", 0);
            // app_id 从 0 开始，kernel_stack_id 从 1 开始
            let task = TaskControlBlock::new("/test/init",1,None);
            //task.task_statut=TaskStatus::Ready; 在new已经设置为ready
            task_deque.push_back(Arc::new(UPSafeCell::new(task)));
            debug!("Application init {} loaded successfully", 0);
        
    
        
        TaskManager {
            task_que_inner: UPSafeCell::new(TaskManagerInner {
                task_queen: task_deque,
                current: 0  // 初始化为第一个任务
            })
        }
    };
}
///返回单个app的内核栈地址（在内核地址空间）
pub fn getapp_kernel_sapce()->usize{
    // 现在内核栈在内核空间，需要返回第0个任务的内核栈顶
    let app_id = 0;  // 第一个任务
    let kernel_stack_bottom = TRAP_BOTTOM_ADDR - (app_id + 1) * (KERNEL_STACK_SIZE + PAGE_SIZE);
    kernel_stack_bottom + KERNEL_STACK_SIZE  // 返回栈顶
}


pub fn run_first_task()->!{
    TASK_MANAER.run_first_task();
}