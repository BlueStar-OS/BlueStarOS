use crate::arch::memory::*;
use crate::arch::task::*;
use crate::arch::trap::TrapContext;
use crate::config::*;
use crate::fs::component::tty::*;
use crate::fs::semaphore::waitqueue::WaitQueue;
use crate::fs::vfs::File;
use crate::kernel_trap_handler;
use crate::memory::*;
use crate::shutdown;
use crate::task::file_loader;
use crate::task::signal::OsSignal;
use crate::task::Signal;
use crate::time::get_time_ns;
use alloc::collections::vec_deque::VecDeque;
use alloc::string::String;
use alloc::string::ToString;
use alloc::sync::Arc;
use alloc::sync::Weak;
use alloc::vec;
use alloc::vec::Vec;
use lazy_static::lazy_static;
use log::debug;
use log::error;
use log::trace;
use log::warn;

///init进程PID
pub const INIT_PID: i32 = 1;
/// TASK_MANAGER是否初始化，防止死循环
pub static mut TASK_MANAGER_INIT: bool = false;
// ── Linux auxiliary vector (auxv) ──
// 栈上布局：argc → argv[] → NULL → envp NULL → auxv[] → AT_NULL → strings
// 每个 auxv 条目 = {key: usize, value: usize}，共 16 字节
// 参考：linux/include/uapi/linux/auxvec.h
const AUX_ENTRY_SIZE: usize = size_of::<usize>() * 2;

const AT_NULL: usize = 0; // auxv 终止符                         [✅ 已实现]
const AT_IGNORE: usize = 1; // 忽略的条目                          [占位 0]
const AT_EXECFD: usize = 2; // 废弃（原 exec 的 fd）               [占位 0]
const AT_PHDR: usize = 3; // 程序头表虚拟地址                    [✅ 已实现]
const AT_PHENT: usize = 4; // 程序头表单项大小                    [✅ 已实现]
const AT_PHNUM: usize = 5; // 程序头表项数                        [✅ 已实现]
const AT_PAGESZ: usize = 6; // 系统页大小                          [✅ 已实现]
const AT_BASE: usize = 7; // 动态链接器基址（静态链接则为 0）    [✅ PIE 前 = 0]
const AT_FLAGS: usize = 8; // 平台标志                            [占位 0]
const AT_ENTRY: usize = 9; // 程序入口                            [✅ 已实现]
const AT_NOTELF: usize = 10; // 内核标记：非 ELF 格式               [占位 0]
const AT_UID: usize = 11; // 真实 UID                            [TODO 无多用户]
const AT_EUID: usize = 12; // 有效 UID                            [TODO 无多用户]
const AT_GID: usize = 13; // 真实 GID                            [TODO 无多用户]
const AT_EGID: usize = 14; // 有效 GID                            [TODO 无多用户]
const AT_PLATFORM: usize = 15; // CPU 型号字符串指针             [TODO 无多用户]
const AT_HWCAP: usize = 16; // RISC-V ISA 扩展位掩码          [TODO 无多用户]
const AT_CLKTCK: usize = 17; // times() 用 ticks/sec            [✅ 已实现 = 100]
const AT_SECURE: usize = 23; // setuid 安全模式标志             [✅ 已实现 = 0]
const AT_BASE_PLATFORM: usize = 24; // 平台字符串                 [占位 0]
const AT_RANDOM: usize = 25; // 16 字节随机数地址（栈 canary） [TODO 无多用户]
const AT_HWCAP2: usize = 26; // RISC-V 不使用                  [占位 0]
const AT_EXECFN: usize = 31; // 可执行文件名字符串指针          [TODO 无多用户]
const AT_SYSINFO_EHDR: usize = 33; // vDSO ELF 头地址            [✅ 已实现 = 0，无 vDSO]

#[repr(C)]
struct AuxKey(usize);
#[repr(C)]
struct AuxValue(usize);
#[repr(C)]
struct AuxEntry {
    key: AuxKey,
    value: AuxValue,
}
impl AuxEntry {
    pub const fn new(key: AuxKey, value: AuxValue) -> Self {
        Self { key, value }
    }
}

/// musl libc 兼容 auxv：从 ELF 解析并构造 Linux 标准 auxiliary vector
///
/// 条目按 musl `__init_libc` 实际读取需求组织，末尾以 AT_NULL 终止。
/// 目前无法实现的项用 0 占位，musl 对缺失项有内部回退逻辑。
fn build_auxv(elf_data: &[u8], elf_entry: usize) -> Vec<AuxEntry> {
    let elf = xmas_elf::ElfFile::new(elf_data).expect("auxv: bad ELF");
    let ehdr = elf.header;
    let ph_count = ehdr.pt2.ph_count() as usize;
    let ph_entry_size = ehdr.pt2.ph_entry_size() as usize;
    let ph_offset = ehdr.pt2.ph_offset() as usize;
    let phdr_end = ph_offset + ph_count * ph_entry_size;

    let mut at_phdr = 0;
    for i in 0..ph_count {
        let ph = elf.program_header(i as u16).unwrap();
        if ph.get_type().unwrap() != xmas_elf::program::Type::Load {
            continue;
        }
        let seg_start = ph.offset() as usize;
        let seg_end = seg_start + ph.file_size() as usize;
        if seg_start <= ph_offset && seg_end >= phdr_end {
            let base = (ph.virtual_addr() as usize).wrapping_sub(seg_start);
            at_phdr = base.wrapping_add(ph_offset);
            break;
        }
    }
    if at_phdr == 0 {
        warn!(
            "auxv: PHDR table (off=0x{:x}, end=0x{:x}) not covered by any LOAD segment, AT_PHDR=0",
            ph_offset, phdr_end
        );
    }

    vec![
        AuxEntry::new(AuxKey(AT_PAGESZ), AuxValue(PAGE_SIZE)),
        AuxEntry::new(AuxKey(AT_PHDR), AuxValue(at_phdr)),
        AuxEntry::new(AuxKey(AT_PHENT), AuxValue(ph_entry_size)),
        AuxEntry::new(AuxKey(AT_PHNUM), AuxValue(ph_count)),
        AuxEntry::new(AuxKey(AT_ENTRY), AuxValue(elf_entry)),
        AuxEntry::new(AuxKey(AT_BASE), AuxValue(0)),
        AuxEntry::new(AuxKey(AT_CLKTCK), AuxValue(TIME_FREQUENT)),
        AuxEntry::new(AuxKey(AT_SECURE), AuxValue(0)),
        AuxEntry::new(AuxKey(AT_HWCAP), AuxValue(0)),
        AuxEntry::new(AuxKey(AT_RANDOM), AuxValue(0)),
        AuxEntry::new(AuxKey(AT_SYSINFO_EHDR), AuxValue(0)),
        AuxEntry::new(AuxKey(AT_UID), AuxValue(0)),
        AuxEntry::new(AuxKey(AT_EUID), AuxValue(0)),
        AuxEntry::new(AuxKey(AT_GID), AuxValue(0)),
        AuxEntry::new(AuxKey(AT_EGID), AuxValue(0)),
        AuxEntry::new(AuxKey(AT_NULL), AuxValue(0)),
    ]
}

use crate::sync::UPSafeCell;

/// 任务运行状态。
#[derive(Clone, PartialEq, Debug)]
pub enum TaskStatus {
    /// 正在 CPU 上执行
    Runing,
    /// 已退出，等待父进程回收
    Zombie,
    /// 阻塞中，等待事件唤醒
    Blocking,
    /// 就绪，在调度队列中等待被选中
    Ready,
}

/// 进程 ID。
#[derive(Clone)]
pub struct ProcessId(pub i32);

/// PID 分配器。
pub struct ProcessIdAlloctor {
    current: i32,
    end: i32,
    id_pool: Vec<ProcessId>,
}

/// 进程控制块。
#[derive(Clone)]
pub struct TaskControlBlock {
    /// 待处理的信号队列
    pub signal: VecDeque<OsSignal>,
    /// 进程 ID
    pub pid: ProcessId,
    /// 地址空间
    pub memory_set: MapSet,
    /// 当前运行状态
    pub task_statut: TaskStatus,
    /// 退出码
    pub exit_code: isize,
    /// 任务上下文
    pub task_context: TaskContext,
    /// 陷阱上下文物理页帧号
    pub trap_context_ppn: usize,
    pass: usize,
    stride: usize,
    ticket: usize,
    /// 文件描述符表
    pub file_descriptor: Vec<Option<Arc<dyn File>>>,
    /// 当前工作目录
    pub cwd: String,
    /// 父进程
    pub parent: Option<Weak<UPSafeCell<TaskControlBlock>>>,
    /// 子进程列表
    pub childrens: Vec<Arc<UPSafeCell<TaskControlBlock>>>,
    /// 进程退出等待队列
    pub exit_queue: Arc<WaitQueue>,
}

/// 任务管理器内部状态。
pub struct TaskManagerInner {
    /// 全局任务队列
    pub task_queen: VecDeque<Arc<UPSafeCell<TaskControlBlock>>>,
    /// 当前任务索引
    pub current: usize,
}

/// 全局任务管理器。
pub struct TaskManager {
    /// 内部任务队列状态
    pub task_que_inner: UPSafeCell<TaskManagerInner>,
}

impl ProcessIdAlloctor {
    /// 初始化进程 ID 分配器。
    pub fn initial_processid_alloctor(start: i32, end: i32) -> Self {
        let id_pool: Vec<ProcessId> = Vec::new();
        ProcessIdAlloctor {
            current: start,
            end,
            id_pool,
        }
    }

    /// 分配进程 ID。
    pub fn alloc_id(&mut self) -> Option<ProcessId> {
        if !self.id_pool.is_empty() {
            return self.id_pool.pop();
        }
        if self.current < self.end {
            self.current += 1;
            return Some(ProcessId(self.current - 1));
        }
        None
    }
}

impl Drop for ProcessId {
    fn drop(&mut self) {
        ProcessId_ALLOCTOR.lock(|alloc| alloc.id_pool.push(ProcessId(self.0)));
        trace!("Process Id :{} recycled!", self.0)
    }
}

impl TaskControlBlock {
    fn align_up(x: usize, align: usize) -> usize {
        (x + align - 1) & !(align - 1)
    }

    fn align_slice(buf: &mut [u8], alignment: usize, cursor: &mut usize) {
        while !(*cursor).is_multiple_of(alignment) {
            buf[*cursor] = 0;
            *cursor += 1;
        }
    }

    /// 将 argv 与 auxv 写入用户初始栈。
    fn push_args_to_user_stack(
        satp: usize,
        user_sp: usize,
        argv: &[String],
        auxv: &mut [AuxEntry],
    ) -> usize {
        let mut tb = PageTable::crate_table_from_satp(satp);
        let mut total_size: usize = 0;
        let mut cursor: usize = 0;
        let atranom_size = size_of::<[u8; 16]>();
        let usize_size = core::mem::size_of::<usize>();
        total_size = total_size.saturating_add(usize_size);
        total_size = total_size.saturating_add(argv.len() * usize_size);
        total_size = total_size.saturating_add(usize_size * 2);
        total_size = total_size.saturating_add(auxv.len() * AUX_ENTRY_SIZE);

        let mut str_area_off: usize = total_size;
        total_size = total_size.saturating_add(atranom_size);

        for arg in argv.iter() {
            let align_size = Self::align_up(arg.len() + 1, usize_size);
            total_size = total_size.saturating_add(align_size);
        }

        let align_pad = (16 - total_size % 16) % 16;
        total_size = total_size.saturating_add(align_pad);

        let mut str_usr_addr = user_sp - total_size + str_area_off;
        let mut stack_blob: Vec<u8> = vec![0u8; total_size];

        cursor += core::mem::size_of::<usize>();
        stack_blob[0..cursor].copy_from_slice(&argv.len().to_le_bytes());
        Self::align_slice(&mut stack_blob, usize_size, &mut cursor);

        for arg in argv.iter() {
            let mut real_arg = arg.clone();
            real_arg.push(0 as char);
            let byte_len = arg.len() + 1;
            stack_blob[cursor..cursor + usize_size].copy_from_slice(&str_usr_addr.to_le_bytes());
            cursor += usize_size;
            stack_blob[str_area_off..str_area_off + byte_len].copy_from_slice(real_arg.as_bytes());

            str_usr_addr += Self::align_up(byte_len, usize_size);
            str_area_off += Self::align_up(byte_len, usize_size);
        }
        cursor += usize_size * 2;

        let magic_number: usize = 1314;
        let time = get_time_ns();
        let last_random = time.saturating_sub(magic_number) / 2 + 8;
        let at_random: [u8; 16] = {
            let src = last_random.to_ne_bytes();
            let mut buf = [0u8; 16];
            let mut i = 0;
            while i < 16 {
                buf[i] = src[i % 8].wrapping_add(1);
                i += 1;
            }
            buf
        };
        let at_random_start = str_area_off;
        let at_random_usr_adr = str_usr_addr;
        stack_blob[at_random_start..at_random_start + 16].copy_from_slice(&at_random);

        auxv.iter_mut()
            .find(|auxv| auxv.key.0 == AT_RANDOM)
            .iter_mut()
            .for_each(|uv| {
                (uv.value.0) = at_random_usr_adr;
            });

        let auxv_raw: &[u8] = unsafe {
            core::slice::from_raw_parts(auxv.as_ptr() as *const u8, auxv.len() * AUX_ENTRY_SIZE)
        };
        stack_blob[cursor..cursor + auxv_raw.len()].copy_from_slice(auxv_raw);

        let pages_needed = stack_blob.len() / PAGE_SIZE;
        let stack_pages_max = KERNEL_STACK_SIZE / PAGE_SIZE;
        let mut page_list: Vec<&mut [u8; PAGE_SIZE]> = Vec::new();
        let first_page = match tb.get_mut_byte(VirAddr(user_sp - total_size).into()) {
            Some(page) => page,
            None => return user_sp,
        };
        page_list.push(first_page);
        if pages_needed > 1 {
            if pages_needed > stack_pages_max {
                return user_sp;
            }
            for i in 1..pages_needed {
                let next_page = match tb
                    .get_mut_byte(VirAddr(user_sp - total_size + PAGE_SIZE * i + 1).into())
                {
                    Some(page) => page,
                    None => return user_sp,
                };
                page_list.push(next_page);
            }
        }

        let mut page_idx = 0;
        for chunk in stack_blob.chunks(PAGE_SIZE) {
            if chunk.is_empty() {
                break;
            }
            if chunk.len() != PAGE_SIZE {
                if page_idx == 0 {
                    page_list[page_idx][PAGE_SIZE - chunk.len()..].copy_from_slice(chunk);
                    break;
                } else {
                    page_list[page_idx][..chunk.len()].copy_from_slice(chunk);
                    break;
                }
            } else {
                page_list[page_idx].copy_from_slice(chunk);
                page_idx += 1;
            }
        }

        user_sp - total_size
    }

    /// 设置父进程引用。
    pub fn set_father(&mut self, father: &Arc<UPSafeCell<TaskControlBlock>>) {
        self.parent = Some(Arc::downgrade(father));
    }

    /// 添加子进程。
    pub fn add_children(&mut self, tlb: Arc<UPSafeCell<TaskControlBlock>>) {
        self.childrens.push(tlb);
    }

    /// 获取当前工作目录。
    pub fn get_cwd(&self) -> &str {
        &self.cwd
    }

    /// 设置当前工作目录。
    pub fn set_cwd(&mut self, cwd: String) {
        self.cwd = cwd;
    }

    /// 用新的 ELF 镜像替换当前进程。
    pub fn new_exec_task(&mut self, path: &str, argv: Vec<String>, argc: usize) -> bool {
        debug!("exec: replacing current task image with {}", path);
        let elf_data = file_loader(path);
        if elf_data.is_empty() {
            return false;
        }
        self.new_exec_task_with_elf(path, argv, argc, &elf_data)
    }

    /// 使用已经加载的 ELF 数据替换当前进程镜像。
    pub fn new_exec_task_with_elf(
        &mut self,
        _path: &str,
        argv: Vec<String>,
        argc: usize,
        elf_data: &[u8],
    ) -> bool {
        debug!("Load success");
        let re = MapSet::from_elf(elf_data);
        if re.is_none() {
            warn!("Can't create elf file");
            return false;
        }
        let (mut memset, elf_entry, user_sp, kernel_sp) = re.expect("Kernel error");
        let task_cx = TaskContext::return_trap_new(kernel_sp);
        let kernel_satp = KERNEL_SPACE.lock(|ks| ks.table.satp_token());
        let user_satp = memset.table.satp_token();
        let trap_cx_ppn = memset
            .table
            .translate_byvpn(VirAddr(TRAP_CONTEXT_ADDR).strict_into_virnum())
            .expect("trap ppn translate failed");

        let _ = argc;
        let mut auxv = build_auxv(elf_data, elf_entry);
        let new_user_sp = Self::push_args_to_user_stack(user_satp, user_sp.0, &argv, &mut auxv);

        self.memory_set = memset;
        self.task_context = task_cx;
        self.trap_context_ppn = trap_cx_ppn.0;

        let trap_cx_point: *mut TrapContext = (trap_cx_ppn.0 * PAGE_SIZE) as *mut TrapContext;
        unsafe {
            *trap_cx_point = TrapContext::init_app_trap_context(
                elf_entry,
                kernel_satp,
                kernel_trap_handler as *const () as usize,
                kernel_sp,
                new_user_sp,
            );
        }
        true
    }

    /// 创建新任务。
    fn new(
        app_path: &str,
        _kernel_stack_id: usize,
        father: Option<Weak<UPSafeCell<TaskControlBlock>>>,
    ) -> Option<Self> {
        debug!(
            "Creating task for app_path: {}, kernel_stack_id: {}",
            app_path, _kernel_stack_id
        );

        let elf_data = file_loader(app_path);
        let re = MapSet::from_elf(&elf_data);
        if re.is_none() {
            warn!("Can't create elf file");
            return None;
        }
        let (mut memset, elf_entry, user_sp, kernel_sp) = re.expect("Kernel error");
        let task_cx = TaskContext::return_trap_new(kernel_sp);
        let kernel_satp = KERNEL_SPACE.lock(|ks| ks.table.satp_token());
        let user_satp = memset.table.satp_token();
        let trap_cx_ppn = memset
            .table
            .translate_byvpn(VirAddr(TRAP_CONTEXT_ADDR).strict_into_virnum())
            .expect("trap ppn translate failed");

        let argv = alloc::vec![alloc::string::String::from(app_path)];
        let mut auxv = build_auxv(&elf_data, elf_entry);
        let new_user_sp = Self::push_args_to_user_stack(user_satp, user_sp.0, &argv, &mut auxv);

        let file_descriptor_table: Vec<Option<Arc<dyn File>>> = vec![
            Some(stdin_file()),
            Some(stdout_file()),
            Some(stderr_file()),
        ];

        let task_control_block = TaskControlBlock {
            signal: VecDeque::new(),
            pid: ProcessId_ALLOCTOR
                .lock(|alloc| alloc.alloc_id())
                .expect("No Process ID Can use"),
            memory_set: memset,
            task_statut: TaskStatus::Ready,
            exit_code: 0,
            task_context: task_cx,
            trap_context_ppn: trap_cx_ppn.0,
            pass: 0,
            stride: BIG_INT / TASK_TICKET,
            ticket: TASK_TICKET,
            file_descriptor: file_descriptor_table,
            cwd: "/".to_string(),
            parent: father,
            childrens: Vec::new(),
            exit_queue: Arc::new(WaitQueue::new()),
        };

        let trap_cx_point: *mut TrapContext = (trap_cx_ppn.0 * PAGE_SIZE) as *mut TrapContext;
        unsafe {
            *trap_cx_point = TrapContext::init_app_trap_context(
                elf_entry,
                kernel_satp,
                kernel_trap_handler as *const () as usize,
                kernel_sp,
                new_user_sp,
            );
        }

        debug!(
            "Task created successfully: entry={:#x}, user_sp={:#x}",
            elf_entry, user_sp.0
        );
        Some(task_control_block)
    }
}

impl Drop for TaskControlBlock {
    fn drop(&mut self) {
        for child in self.childrens.iter() {
            child.lock(|c| c.parent = None);
        }
        self.childrens.clear();
    }
}

impl TaskManager {
    /// 阻塞当前任务并切换到下一个就绪任务。
    pub fn block_and_switch(&self) {
        let result = self.task_que_inner.lock(|inner| {
            let current = inner.current;
            let selected = Self::stride_select_task_inner(inner);
            let (selected_idx, _) = match selected {
                Some(s) => s,
                None => {
                    inner.task_queen[current].lock(|t| t.task_statut = TaskStatus::Ready);
                    return None;
                }
            };

            let swap_out =
                inner.task_queen[current].lock(|t| &mut t.task_context as *mut TaskContext);
            inner.task_queen.remove(current);

            let adjusted = if selected_idx > current {
                selected_idx - 1
            } else {
                selected_idx
            };

            inner.current = adjusted;
            let swap_in = inner.task_queen[adjusted].lock(|t| {
                t.task_statut = TaskStatus::Runing;
                &mut t.task_context as *mut TaskContext
            });

            Some((swap_out, swap_in))
        });

        if let Some((swap_out, swap_in)) = result {
            unsafe {
                __switch(swap_out, swap_in);
            }
        }
    }

    /// 处理当前任务的信号。
    pub fn resolve_current_task_signal(&self) {
        let (current_task, pid) = self.task_que_inner.lock(|inner| {
            if inner.task_queen.is_empty() {
                return (None, 0);
            }
            let current = inner.current;
            if current >= inner.task_queen.len() {
                return (None, 0);
            }
            let task = inner.task_queen[current].clone();
            let pid = task.lock(|t| t.pid.0);
            (Some(task), pid)
        });

        let Some(current_task) = current_task else {
            return;
        };

        let signal: VecDeque<OsSignal> = current_task.lock(|t| t.signal.drain(..).collect());
        if signal.is_empty() {
            return;
        }

        for sig in signal {
            crate::kprintln!("[Signal] PID {} reviec signal {:?}", pid, sig.signal);
            match sig.signal {
                Signal::SIGKILL | Signal::SIGINT | Signal::SIGTERM => {
                    TASK_MANAER.kail_current_task_and_run_next();
                }
                Signal::SIGTSTP => {}
                _ => warn!("未处理的信号: {:?}", sig.signal),
            }
        }
    }

    /// 将当前任务标记为 Zombie 并记录退出码。
    pub fn mark_current_zombie(&self, exit_code: isize) {
        self.task_que_inner.lock(|inner| {
            if inner.task_queen.is_empty() {
                panic!("Task Queen is empty!");
            }
            let current = inner.current;
            if current >= inner.task_queen.len() {
                panic!("TaskManager current index out of range");
            }
            inner.task_queen[current].lock(|t| {
                t.task_statut = TaskStatus::Zombie;
                t.exit_code = exit_code;
            });
        });
    }

    /// 将当前任务的子进程过继给 init。
    pub fn reparent_current_children_to_init(&self) {
        let (current_task, init_task) = self.task_que_inner.lock(|inner| {
            if inner.task_queen.is_empty() {
                return (None, None);
            }
            let current = inner.current;
            if current >= inner.task_queen.len() {
                return (None, None);
            }
            let current_task = inner.task_queen[current].clone();
            let init_task = inner
                .task_queen
                .iter()
                .find(|t| t.lock(|t| t.pid.0 == INIT_PID))
                .cloned();
            (Some(current_task), init_task)
        });

        let Some(current_task) = current_task else {
            return;
        };
        let Some(init_task) = init_task else {
            return;
        };

        if current_task.lock(|t| t.pid.0 == INIT_PID) {
            warn!("Init are exit: pid {} ", INIT_PID);
            return;
        }
        let children = current_task.lock(|t| core::mem::take(&mut t.childrens));
        if children.is_empty() {
            return;
        }
        let init_weak = Arc::downgrade(&init_task);
        init_task.lock(|init| {
            for child in children.iter() {
                init.childrens.push(child.clone());
            }
        });
        for child in children {
            child.lock(|c| c.parent = Some(init_weak.clone()));
        }
    }

    /// 回收指定 PID 的 Zombie 子进程并返回退出码。
    pub fn reap_zombie_child(&self, child_pid: i32) -> Option<isize> {
        self.task_que_inner.lock(|inner| {
            let current = inner.current;
            if inner.task_queen.is_empty() || current >= inner.task_queen.len() {
                return None;
            }
            let current_task = inner.task_queen[current].clone();

            let is_child = current_task.lock(|parent| {
                parent
                    .childrens
                    .iter()
                    .any(|c| c.lock(|c| c.pid.0 == child_pid))
            });
            if !is_child {
                return None;
            }

            let mut zombie_index: Option<usize> = None;
            let mut exit_code: Option<isize> = None;
            for (idx, cell) in inner.task_queen.iter().enumerate() {
                let done = cell.lock(|t| {
                    if t.pid.0 == child_pid {
                        if matches!(t.task_statut, TaskStatus::Zombie) {
                            zombie_index = Some(idx);
                            exit_code = Some(t.exit_code);
                        }
                        true
                    } else {
                        false
                    }
                });
                if done {
                    break;
                }
            }
            let idx = zombie_index?;

            inner.task_queen.remove(idx);
            if !inner.task_queen.is_empty() {
                if inner.current > idx {
                    inner.current -= 1;
                } else if inner.current >= inner.task_queen.len() {
                    inner.current = 0;
                }
            }

            current_task.lock(|parent| {
                parent
                    .childrens
                    .retain(|c| c.lock(|c| c.pid.0 != child_pid));
            });

            exit_code
        })
    }

    /// 根据路径加载新任务到任务管理器。
    pub fn load_newtask_to_taskmanager(_path: &str) {}

    /// 添加任务或将任务重新加入队列。
    pub fn add_task(self, task: Arc<UPSafeCell<TaskControlBlock>>) {
        self.task_que_inner
            .lock(|inner| inner.task_queen.push_back(task));
    }

    /// 根据 Stride 选择任务。
    pub fn stride_select_task(&self, inner: &TaskManagerInner) -> Option<(usize, usize)> {
        Self::stride_select_task_inner(inner)
    }

    fn stride_select_task_inner(inner: &TaskManagerInner) -> Option<(usize, usize)> {
        let current = inner.current;
        let mut selected: Option<(usize, usize)> = None;

        let has_ready = inner
            .task_queen
            .iter()
            .any(|t| t.lock(|t| t.task_statut == TaskStatus::Ready));
        if !has_ready {
            for task in inner.task_queen.iter() {
                let is_init = task.lock(|t| t.pid.0 == INIT_PID);
                if is_init {
                    task.lock(|t| t.task_statut = TaskStatus::Ready);
                    break;
                }
            }
        }

        for (idx, cell) in inner.task_queen.iter().enumerate() {
            cell.lock(|t| {
                if let TaskStatus::Ready = t.task_statut {
                    let pass = t.pass;
                    match selected {
                        Some((best_idx, best_pass)) => {
                            if pass < best_pass
                                || (pass == best_pass && best_idx == current && idx != current)
                            {
                                selected = Some((idx, pass));
                            }
                        }
                        None => selected = Some((idx, pass)),
                    }
                }
            });
        }
        selected
    }

    /// 从队列移除当前任务。
    pub fn remove_current_task(&self) {
        let (removed_task, init_task) = self.task_que_inner.lock(|inner| {
            let task_to_remove = inner.current;
            debug!(
                "Removing task at index: {}, queue length before removal: {}",
                task_to_remove,
                inner.task_queen.len()
            );

            let removed_task = inner.task_queen[task_to_remove].clone();
            let init_task = inner
                .task_queen
                .iter()
                .find(|t| t.lock(|t| t.pid.0 == INIT_PID))
                .cloned();

            inner
                .task_queen
                .remove(task_to_remove)
                .expect("Remove Task Control Block Failed!");

            if !inner.task_queen.is_empty() {
                if task_to_remove > inner.task_queen.len() {
                    panic!(
                        "Task Remove Faild , kernel try to remove a task that index over taskqueen"
                    )
                }
                if task_to_remove == inner.task_queen.len() {
                    if let Some(select_task) = Self::stride_select_task_inner(inner) {
                        inner.current = select_task.0;
                    } else {
                        error!("After remove,No task can select");
                        shutdown();
                    }
                } else {
                    inner.current = task_to_remove;
                }
                debug!(
                    "After removal: current set to {}, queue length: {}",
                    inner.current,
                    inner.task_queen.len()
                );
            }

            (removed_task, init_task)
        });

        if let Some(init_task) = init_task {
            if removed_task.lock(|t| t.pid.0) != INIT_PID {
                let children = removed_task.lock(|t| core::mem::take(&mut t.childrens));
                if !children.is_empty() {
                    let init_weak = Arc::downgrade(&init_task);
                    init_task.lock(|init| {
                        for child in children.iter() {
                            init.childrens.push(child.clone());
                        }
                    });
                    for child in children {
                        child.lock(|c| c.parent = Some(init_weak.clone()));
                    }
                }
            }
        } else {
            error!("Can't find init process!,these child process will become foster process");
            let children = removed_task.lock(|t| core::mem::take(&mut t.childrens));
            for child in children {
                child.lock(|c| c.parent = None);
            }
        }

        if self.task_queen_is_empty() {
            error!("The last task(should be init) exit or be removed,shutdown");
            shutdown();
        }
    }

    /// 挂起当前任务并通过 Stride 调度下一个 READY 任务。
    pub fn suspend_and_run_task(&self) {
        if self.task_queen_is_empty() {
            panic!("Task Queen is empty!");
        }

        let (swap_out, _current, task_index) = self.task_que_inner.lock(|inner| {
            if inner.task_queen.is_empty() {
                panic!("Task Queen is empty!");
            }
            let current = inner.current;
            if current >= inner.task_queen.len() {
                panic!("TaskManager current index out of range");
            }

            inner.task_queen[current].lock(|cur| {
                if !matches!(cur.task_statut, TaskStatus::Zombie) {
                    cur.task_statut = TaskStatus::Ready;
                    cur.pass += cur.stride;
                }
            });

            let task_index = match Self::stride_select_task_inner(inner) {
                Some((idx, _)) => idx,
                None => {
                    warn!("No task can select");
                    shutdown()
                }
            };

            if task_index >= inner.task_queen.len() {
                panic!("Selected task index out of range");
            }

            inner.task_queen[task_index].lock(|t| {
                if !matches!(t.task_statut, TaskStatus::Ready) {
                    panic!("Selected task is not Ready");
                }
            });

            if current == task_index {
                inner.task_queen[task_index].lock(|t| {
                    t.task_statut = TaskStatus::Runing;
                });
                return (core::ptr::null_mut(), current, task_index);
            }

            let swap_out =
                inner.task_queen[current].lock(|cur| &mut cur.task_context as *mut TaskContext);

            (swap_out, current, task_index)
        });

        if swap_out.is_null() {
            return;
        }

        let swap_in = self.task_que_inner.lock(|inner| {
            inner.current = task_index;
            inner.task_queen[task_index].lock(|next| {
                next.task_statut = TaskStatus::Runing;
                &mut next.task_context as *mut TaskContext
            })
        });

        unsafe {
            __switch(swap_out, swap_in);
        }
    }

    /// 检查任务队列是否为空。
    pub fn task_queen_is_empty(&self) -> bool {
        self.task_que_inner
            .lock(|inner| inner.task_queen.is_empty())
    }

    /// 运行第一个任务。
    pub fn run_first_task(&self) -> ! {
        let task_cx_ptr = self.task_que_inner.lock(|inner| {
            let idx = inner.current;
            inner.task_queen[idx].lock(|task| {
                task.task_statut = TaskStatus::Runing;
                task.pass += task.stride;
                &mut task.task_context as *mut TaskContext
            })
        });
        let kernel_task_cx = TaskContext::zero_init();

        debug!("run_first_task: task_cx_ptr={:#x}", task_cx_ptr as usize);
        debug!(
            "run_first_task: task sp={:#x}, lr(x30)={:#x}",
            unsafe { (*task_cx_ptr).kernel_sp },
            unsafe { core::ptr::read((task_cx_ptr as *const u8).add(11 * 8) as *const usize) }
        );

        unsafe {
            __switch(&kernel_task_cx as *const TaskContext, task_cx_ptr);
        }

        panic!("unreachable in run_first_task!");
    }

    /// 删除当前任务后直接切换到 inner.current 指向的任务。
    pub fn run_current_task(&self) -> ! {
        let task_cx_ptr = self.task_que_inner.lock(|inner| {
            if inner.task_queen.is_empty() {
                panic!("Task Queen is empty!");
            }
            let current = inner.current;
            if current >= inner.task_queen.len() {
                panic!("TaskManager current index out of range");
            }
            inner.task_queen[current].lock(|task| {
                task.task_statut = TaskStatus::Runing;
                task.pass += task.stride;
                &mut task.task_context as *mut TaskContext
            })
        });
        let dummy = TaskContext::zero_init();
        unsafe {
            __switch(&dummy as *const TaskContext, task_cx_ptr);
        }
        panic!("unreachable in run_current_task!");
    }

    /// 获取当前任务页表 SATP token。
    pub fn get_current_stap(&self) -> usize {
        self.task_que_inner.lock(|inner| {
            let current = inner.current;
            inner.task_queen[current].lock(|task| task.memory_set.get_table().satp_token())
        })
    }

    /// 获取当前任务陷阱上下文的可变引用。
    #[allow(clippy::mut_from_ref)]
    pub fn get_current_trapcx(&self) -> &mut TrapContext {
        let task_trap_ppn = self.task_que_inner.lock(|inner| {
            let current = inner.current;
            inner.task_queen[current].lock(|task| task.trap_context_ppn)
        });
        let origin_phyaddr = (task_trap_ppn * PAGE_SIZE) as *mut TrapContext;
        unsafe { &mut *origin_phyaddr }
    }

    /// 获取当前任务的文件描述符。
    pub fn get_current_fd(&self, fd: usize) -> Option<Option<Arc<dyn File>>> {
        self.task_que_inner.lock(|inner| {
            let current = inner.current;
            inner.task_queen[current].lock(|task| task.file_descriptor.get(fd).cloned())
        })
    }

    /// 获取当前任务的工作目录。
    pub fn get_current_cwd(&self) -> String {
        self.task_que_inner.lock(|inner| {
            if inner.task_queen.is_empty() {
                return "/".to_string();
            }
            let current = inner.current;
            inner.task_queen[current].lock(|task| task.get_cwd().to_string())
        })
    }

    /// 设置当前任务的工作目录。
    pub fn set_current_cwd(&self, cwd: String) {
        self.task_que_inner.lock(|inner| {
            let current = inner.current;
            inner.task_queen[current].lock(|task| {
                task.set_cwd(cwd);
            });
        });
    }

    /// 为当前任务分配空闲 fd 并安装文件对象。
    pub fn alloc_fd_for_current(&self, new_fd: Arc<dyn File>) -> i32 {
        self.task_que_inner.lock(|inner| {
            let current = inner.current;
            inner.task_queen[current].lock(|task| {
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
            })
        })
    }

    /// 关闭当前任务的指定 fd。
    pub fn close_current_fd(&self, fd: usize) -> isize {
        self.task_que_inner.lock(|inner| {
            let current = inner.current;
            inner.task_queen[current].lock(|task| {
                if fd >= task.file_descriptor.len() {
                    return -1;
                }
                if task.file_descriptor[fd].is_none() {
                    return -1;
                }
                let fdd = task.file_descriptor[fd].take();

                if !fdd.expect("the fd not exist").close().unwrap() {
                    error!("The fd close failed!");
                }

                if task.file_descriptor[fd].is_none() {
                    0
                } else {
                    error!("Close fd :{} failed!", fd);
                    -1
                }
            })
        })
    }

    /// 终止当前任务并运行下一个任务。
    pub fn kail_current_task_and_run_next(&self) {
        use crate::syscall::sys_exit::sys_exit;
        sys_exit(usize::MAX);
        error!("Task Kailed!");
    }
}

lazy_static! {
    /// 全局进程 ID 分配器。
    pub static ref ProcessId_ALLOCTOR: UPSafeCell<ProcessIdAlloctor> =
        UPSafeCell::new(ProcessIdAlloctor::initial_processid_alloctor(1, 10_000_000));
}

lazy_static! {
    /// 全局任务管理器，启动时从根文件系统加载 `/test/init`。
    pub static ref TASK_MANAER: TaskManager = {
        debug!("Initializing TASK_MANAGER...");

        let mut task_deque = VecDeque::new();
        let task = TaskControlBlock::new("/test/init", 1, None).expect("Can't load init elf");
        task_deque.push_back(Arc::new(UPSafeCell::new(task)));
        debug!("Application init loaded successfully");

        unsafe {
            TASK_MANAGER_INIT = true;
        }

        TaskManager {
            task_que_inner: UPSafeCell::new(TaskManagerInner {
                task_queen: task_deque,
                current: 0,
            }),
        }
    };
}

/// 获取第一个任务的内核栈顶地址。
pub fn getapp_kernel_sapce() -> usize {
    let app_id = 0;
    let kernel_stack_bottom = TRAP_BOTTOM_ADDR - (app_id + 1) * (KERNEL_STACK_SIZE + PAGE_SIZE);
    kernel_stack_bottom + KERNEL_STACK_SIZE
}

/// 启动第一个任务。
pub fn run_first_task() -> ! {
    TASK_MANAER.run_first_task()
}
