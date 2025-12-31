
use core::mem::size_of;
use alloc::collections::vec_deque::VecDeque;
use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use alloc::string::String;
use log::{debug, error, warn};
use crate::sbi::shutdown;
use crate::sync::UPSafeCell;
use crate::task::{ProcessId, TaskStatus, INIT_PID};
use crate::{config::PAGE_SIZE, memory::{PageTable, VirAddr, VirNumber}, task::TASK_MANAER, time::{TimeVal, get_time_ms}};
use alloc::vec;
use crate::memory::MapSet;
use crate::fs::vfs::VfsFsError;
use crate::fs::vfs::{vfs_getdents64, vfs_mkdir, vfs_open, vfs_stat, vfs_unlink, OpenFlags, VfsStat};
use crate::fs::vfs::FileDescriptor;
use crate::fs::component::pipe::pipe::{make_pipe, PipeHandle};
use crate::trap::TrapContext;
use crate::TRAP_CONTEXT_ADDR;
use crate::task::ProcessId_ALLOCTOR;
use crate::task::TaskContext;
use crate::alloc::string::ToString;

///SYS_EXEC系统调用
/// argv 命令行字符串参数数组起始地址
/// argc 参数个数
pub fn sys_exec(path_ptr: usize,argv:usize,argc:usize)->isize{
    // 关键：先拿到 satp，再去 lock 当前 task，避免二次借用。
    let user_satp = TASK_MANAER.get_current_stap();

    debug!("sys_exec: path_ptr={:#x} argv_ptr={:#x} argc={} satp={:#x}", path_ptr, argv, argc, user_satp);

    let path = match read_c_string_from_user_with_satp(user_satp, path_ptr) {
        Ok(p) => p,
        Err(_) => return -1,
    };

    // 从用户地址空间读取 argv 指针数组（usize[]）
    let argv_bytes_len = argc.saturating_mul(core::mem::size_of::<usize>());
    let mut slices = PageTable::get_mut_slice_from_satp(user_satp, argv_bytes_len, VirAddr(argv));
    let mut flat: Vec<u8> = Vec::with_capacity(argv_bytes_len);
    for s in slices.iter_mut() {
        flat.extend_from_slice(s);
    }
    if flat.len() < argv_bytes_len {
        error!("sys_exec: short read argv array: need={} got={}", argv_bytes_len, flat.len());
        return -1;
    }

    let mut exec_argv:Vec<String> =Vec::new();
    for i in 0..argc {
        let base = i * core::mem::size_of::<usize>();
        let ptr_bytes: [u8; core::mem::size_of::<usize>()] = flat[base..base + core::mem::size_of::<usize>()]
            .try_into()
            .unwrap();
        let cptr = usize::from_ne_bytes(ptr_bytes);
            debug!("sys_exec: argv[{}] ptr={:#x}", i, cptr);
        match read_c_string_from_user_with_satp(user_satp, cptr) {
            Ok(s) => {
                debug!("sys_exec: argv[{}] = '{}'", i, s);
                exec_argv.push(s)
            }
            Err(e) => {
                error!("sys_exec: Can't translate command string argv[{}] ptr={:#x} err={}", i, cptr, e);
                return -1;
            }
        }
    }

    let current_task = {
        let inner = TASK_MANAER.task_que_inner.lock();
        inner.task_queen[inner.current].clone()
    };
    {
        let mut tcb = current_task.lock();
        if !tcb.new_exec_task(&path,exec_argv,argc) {
            return -1;
        }
    }
    0
}

pub fn sys_pipe(fds_ptr: usize) -> isize {
    if fds_ptr == 0 {
        return -1;
    }

    let (read_end, write_end) = make_pipe();
    let read_fd = Arc::new(FileDescriptor::new_from_inner(
        OpenFlags::RDONLY,
        false,
        Box::new(PipeHandle::new(read_end)),
    ));
    let write_fd = Arc::new(FileDescriptor::new_from_inner(
        OpenFlags::WRONLY,
        false,
        Box::new(PipeHandle::new(write_end)),
    ));

    let rfd = TASK_MANAER.alloc_fd_for_current(read_fd);
    if rfd < 0 {
        return -1;
    }
    let wfd = TASK_MANAER.alloc_fd_for_current(write_fd);
    if wfd < 0 {
        return -1;
    }

    let user_satp = TASK_MANAER.get_current_stap();
    let mut slices = PageTable::get_mut_slice_from_satp(
        user_satp,
        core::mem::size_of::<usize>() * 2,
        VirAddr(fds_ptr),
    );

    let mut tmp: [u8; core::mem::size_of::<usize>() * 2] = [0u8; core::mem::size_of::<usize>() * 2];
    tmp[..core::mem::size_of::<usize>()].copy_from_slice(&(rfd as usize).to_ne_bytes());
    tmp[core::mem::size_of::<usize>()..].copy_from_slice(&(wfd as usize).to_ne_bytes());

    let mut off = 0usize;
    for s in slices.iter_mut() {
        if off >= tmp.len() {
            break;
        }
        let n = core::cmp::min(s.len(), tmp.len() - off);
        s[..n].copy_from_slice(&tmp[off..off + n]);
        off += n;
    }
    if off != tmp.len() {
        return -1;
    }
    0
}

pub fn sys_mkdir(path_ptr: usize) -> isize {
    let path = match read_c_string_from_user(path_ptr) {
        Ok(p) => p,
        Err(e) => {
            error!("sys_mkdir: invalid user path ptr={:#x}, err={}", path_ptr, e);
            return -1;
        }
    };
    match vfs_mkdir(&path) {
        Ok(_) => 0,
        Err(e) => {
            error!("sys_mkdir: vfs_mkdir failed: path={} err={}", path, e);
            -1
        }
    }
}

pub fn sys_unlink(path_ptr: usize) -> isize {
    let path = match read_c_string_from_user(path_ptr) {
        Ok(p) => p,
        Err(e) => {
            error!("sys_unlink: invalid user path ptr={:#x}, err={}", path_ptr, e);
            return -1;
        }
    };
    match vfs_unlink(&path) {
        Ok(_) => 0,
        Err(e) => {
            error!("sys_unlink: vfs_unlink failed: path={} err={}", path, e);
            -1
        }
    }
}

pub fn sys_stat(path_ptr: usize, stat_buf_ptr: usize) -> isize {
    let path = match read_c_string_from_user(path_ptr) {
        Ok(p) => p,
        Err(e) => {
            error!("sys_stat: invalid user path ptr={:#x}, err={}", path_ptr, e);
            return -1;
        }
    };

    let st = match vfs_stat(&path) {
        Ok(s) => s,
        Err(e) => {
            error!("sys_stat: vfs_stat failed: path={} err={}", path, e);
            return -1;
        }
    };

    if stat_buf_ptr == 0 {
        error!("sys_stat: null stat_buf_ptr for path={}", path);
        return -1;
    }

    let user_satp = TASK_MANAER.get_current_stap();
    let mut slices = PageTable::get_mut_slice_from_satp(
        user_satp,
        core::mem::size_of::<VfsStat>(),
        VirAddr(stat_buf_ptr),
    );

    let bytes: &[u8] = unsafe {
        core::slice::from_raw_parts((&st as *const VfsStat) as *const u8, core::mem::size_of::<VfsStat>())
    };
    let mut off = 0usize;
    for s in slices.iter_mut() {
        if off >= bytes.len() {
            break;
        }
        let n = core::cmp::min(s.len(), bytes.len() - off);
        s[..n].copy_from_slice(&bytes[off..off + n]);
        off += n;
    }
    if off != bytes.len() {
        error!("sys_stat: short copy to user: path={} copied={} need={}", path, off, bytes.len());
        return -1;
    }
    0
}

pub fn sys_getdents64(fd: usize, user_buf_ptr: usize, len: usize) -> isize {
    if user_buf_ptr == 0 {
        warn!("sys_getdents64: null user_buf_ptr fd={} len={}", fd, len);
        return -1;
    }
    let file = match TASK_MANAER.get_current_fd(fd) {
        Some(Some(f)) => f,
        _ => {
            warn!("sys_getdents64: invalid fd={} len={}", fd, len);
            return -1;
        }
    };

    let data = match vfs_getdents64(&file, len) {
        Ok(v) => v,
        Err(e) => {
            error!("sys_getdents64: vfs_getdents64 failed fd={} len={} err={}", fd, len, e);
            return -1;
        }
    };

    let user_satp = TASK_MANAER.get_current_stap();
    let mut slices = PageTable::get_mut_slice_from_satp(user_satp, data.len(), VirAddr(user_buf_ptr));
    let mut off = 0usize;
    for s in slices.iter_mut() {
        if off >= data.len() {
            break;
        }
        let n = core::cmp::min(s.len(), data.len() - off);
        s[..n].copy_from_slice(&data[off..off + n]);
        off += n;
    }
    if off != data.len() {
        error!("sys_getdents64: short copy to user fd={} copied={} need={}", fd, off, data.len());
        return -1;
    }
    data.len() as isize
}

pub fn sys_open(path_ptr: usize, flags_bits: usize) -> isize {
    let path = match read_c_string_from_user(path_ptr) {
        Ok(p) => p,
        Err(e) => {
            error!("sys_open: invalid user path ptr={:#x}, err={}", path_ptr, e);
            return -1;
        }
    };

    let acc = flags_bits & 0b11;
    let mut flags = match acc {
        0 => OpenFlags::RDONLY,
        1 => OpenFlags::WRONLY,
        2 => OpenFlags::RDWR,
        _ => {
            error!(
                "sys_open: invalid acc bits: path={} flags_bits={:#x}",
                path, flags_bits
            );
            return -1;
        }
    };
    if (flags_bits & (1 << 6)) != 0 {
        flags.create = true;
    }
    if (flags_bits & (1 << 9)) != 0 {
        flags.truncate = true;
    }
    if (flags_bits & (1 << 10)) != 0 {
        flags.append = true;
    }

    let opened = match vfs_open(&path, flags) {
        Ok(r) => r,
        Err(e) => {
            error!(
                "sys_open: vfs_open failed: path={} flags_bits={:#x} err={}",
                path, flags_bits, e
            );
            return -1;
        }
    };
    let fd = TASK_MANAER.alloc_fd_for_current(opened.fd);
    if fd < 0 {
        error!("sys_open: alloc fd failed: path={} flags_bits={:#x}", path, flags_bits);
    }
    fd
}

pub fn sys_creat(path_ptr: usize) -> isize {
    let flags_bits = (1 << 6) | (1 << 9) | 1;
    sys_open(path_ptr, flags_bits)
}

pub fn sys_close(fd: usize) -> isize {
    let ret = TASK_MANAER.close_current_fd(fd);
    if ret < 0 {
        warn!("sys_close: invalid fd={}", fd);
    }
    ret
}

pub fn sys_lseek(fd: usize, offset: isize, whence: usize) -> isize {
    let file = match TASK_MANAER.get_current_fd(fd) {
        Some(Some(f)) => f,
        _ => {
            warn!("sys_lseek: invalid fd={} offset={} whence={}", fd, offset, whence);
            return -1;
        }
    };
    match file.lseek(offset, whence) {
        Ok(off) => off as isize,
        Err(e) => {
            error!(
                "sys_lseek: failed fd={} offset={} whence={} err={}",
                fd, offset, whence, e
            );
            -1
        }
    }
}


///SYS_FORK系统调用
pub fn sys_fork()->isize{
    let mut inner = TASK_MANAER.task_que_inner.lock();
    let current_index = inner.current;
    let current_task = &mut inner.task_queen[current_index];

    // 先从父进程深拷贝一份新的地址空间（全量复制，不是 COW）
    // clone_mapset 目前签名是 &mut self，所以这里需要拿到父进程的可变 guard。
    let new_memset = {
        let mut parent = current_task.lock();
        parent.memory_set.clone_mapset()
    };


    // 先把浅拷贝得到的 MapSet 用 mem::replace 取出来并 forget，避免 Drop。
    let parent_pid = { current_task.lock().pid.0 };
    let mut bad_task = current_task.lock().clone();//复制的是tbl本体不是arc

    let new_pid = ProcessId_ALLOCTOR
        .lock()
        .alloc_id()
        .expect("No Process ID Can use");
    // 不要让旧的 ProcessId(parent_pid) drop 回收 parent_pid，否则会污染 pid 池。
    let old_pid = core::mem::replace(&mut bad_task.pid, new_pid);
    core::mem::forget(old_pid);
    let child_pid = bad_task.pid.0;
    debug!("Parent:pid {} child:{}", parent_pid, child_pid);
    let shallow = core::mem::replace(&mut bad_task.memory_set, MapSet::new_bare());
    core::mem::forget(shallow);
    bad_task.memory_set = new_memset;

    // 为子进程分配独立的内核栈，并同步到 TaskContext/TrapContext
    let child_kernel_sp = MapSet::alloc_kernel_stack();
    // 子进程第一次被调度必须从 app_entry_point 起步，才能通过 __restore 使用 TrapContext 恢复用户态寄存器。
    // 只修改 sp 会让子进程继承父进程的内核执行流，导致 fork 返回值等寄存器语义错误。
    bad_task.task_context = TaskContext::return_trap_new(child_kernel_sp);
    bad_task.task_statut = TaskStatus::Ready;//设置任务准备被调度
    {
        let trap_cx_ppn = bad_task
        .memory_set
        .table
        .translate_byvpn(VirAddr(TRAP_CONTEXT_ADDR).strict_into_virnum())
        .expect("trap ppn translate failed");
        bad_task.trap_context_ppn = trap_cx_ppn.0;
        let trap_cx_point: *mut TrapContext = (trap_cx_ppn.0 * PAGE_SIZE) as *mut TrapContext;
        unsafe {
            (*trap_cx_point).kernel_sp = child_kernel_sp;
            (*trap_cx_point).x[10] = 0;
            debug!(
                "fork child init: pid={} trap_ppn={} child_a0={}",
                child_pid,
                trap_cx_ppn.0,
                (*trap_cx_point).x[10]
            );
        }
    }

    let arc_task =Arc::new(UPSafeCell::new(bad_task));
    /* 建立父子关系 */
    //添加child
    current_task.lock().add_children( arc_task.clone());
    //链接父亲
    arc_task.lock().set_father(&*current_task);
    drop(inner);//释放TASK_MANAER锁

    /* 把克隆后的任务添加到任务队列 */
    TASK_MANAER.task_que_inner.lock().task_queen.push_back(arc_task.clone());

    //父亲返回子pid，子返回0.
    return child_pid as isize;



}





/// 从用户空间读取 null 结尾的 C 风格字符串
/// 最大读取长度为 4096 字节，避免读取过长的字符串
fn read_c_string_from_user(path_ptr: usize) -> Result<String, VfsFsError> {
    // 获取当前任务的页表
    let user_satp = TASK_MANAER.get_current_stap();
    read_c_string_from_user_with_satp(user_satp, path_ptr)
}

fn read_c_string_from_user_with_satp(user_satp: usize, path_ptr: usize) -> Result<String, VfsFsError> {
    const MAX_PATH_LEN: usize = 4096;

    let buffer = PageTable::get_mut_slice_from_satp(user_satp, MAX_PATH_LEN, VirAddr(path_ptr));
    let mut data: Vec<u8> = Vec::new();
    for slice in buffer {
        data.extend_from_slice(slice);
        if data.len() >= MAX_PATH_LEN {
            break;
        }
    }

    let null_pos = data
        .iter()
        .position(|&b| b == 0)
        .ok_or(VfsFsError::FsInnerError)?;

    let s = core::str::from_utf8(&data[..null_pos])
        .map_err(|_| VfsFsError::FsInnerError)?
        .to_string();
    Ok(s)
}






///mmap系统调用
/// startaddr:usize size:长度
pub fn sys_map(start:usize,size:usize)->isize{
    let inner=TASK_MANAER.task_que_inner.lock();
    let current=inner.current;
    drop(inner);
    let mut inner=TASK_MANAER.task_que_inner.lock();
    let mut memset=&mut inner.task_queen[current].lock().memory_set;
    memset.mmap(VirAddr(start), size)
    //inner自动销毁
}

///unmap系统调用
/// startaddr:usize size:长度
pub fn sys_unmap(start:usize,size:usize)->isize{
    let inner=TASK_MANAER.task_que_inner.lock();
    let current=inner.current;
    drop(inner);
    let inner=TASK_MANAER.task_que_inner.lock();
    let resu:isize;//返回值
    {
        let memset=&mut inner.task_queen[current].lock().memory_set;
        debug!("SYSCALL_UNMAP:ADDR{:#x} LEN:{}",start,size);
        resu=memset.unmap_range(VirAddr(start), size);
    }
    //销毁inner,也可以自动销毁
    drop(inner);
    resu
}



///addr:用户传入的时间结构体地址 目前映射处理错误，因为还没有任务这个概念
fn syscall_get_time(addr:*mut TimeVal){  //考虑是否跨页面  
      let vpn=(addr as usize)/PAGE_SIZE;
      let offset=VirAddr(addr as usize).offset();
      // 获取当前页表的临时视图
      let mut table = PageTable::get_kernel_table_layer();
      let  frame_pointer=table.get_mut_byte(VirNumber(vpn)).expect("Big Error!");

   //判断是否跨页 跨页需要特殊处理
   let len=size_of::<TimeVal>();
   if vpn !=(addr as usize -1 +len)/PAGE_SIZE{
      //跨页
      //let new_frame_pointer=table.get_mut_byte(VirNumber(vpn+1)); 不重复申请，节省内存
      if table.is_maped(VirNumber(vpn+1)){
         //并且存在合法映射,拼接两个页面
        let  time_val:&mut TimeVal;
         unsafe {
           time_val= &mut *((frame_pointer as *mut _ as usize+offset) as *mut TimeVal);
            *time_val=TimeVal{
               sec:get_time_ms()/1000,
               ms:get_time_ms()
            }
         }
      }else { 
          //PageFault!!!!!! 下一个页面没有有效映射
          panic!("InValid Memory write!!")
      }
      
   }


}
///这个指针是用户空间的指针，应该解地址
/// 使用文件描述符进行写入
pub fn sys_write(fd_target: usize, source_buffer: usize, buffer_len: usize) -> isize {
    // 获取当前任务的页表进行地址转换
    let user_satp = TASK_MANAER.get_current_stap();
    let buffer = PageTable::get_mut_slice_from_satp(user_satp, buffer_len, VirAddr(source_buffer));
    
    // 计算总长度并准备写入缓冲区
    let total_len: usize = buffer.iter().map(|slic| slic.len()).sum();
    let mut write_buffer = Vec::with_capacity(total_len);
    
    // 将用户空间的数据复制到内核缓冲区
    for slice in buffer {
        write_buffer.extend_from_slice(slice);
    }

    let fd = match TASK_MANAER.get_current_fd(fd_target) {
        Some(Some(fd)) => fd,
        _ => {
            warn!("sys_write: invalid fd={} len={}", fd_target, buffer_len);
            return -1;
        }
    };

    match fd.write(&write_buffer) {
        Ok(written) => written as isize,
        Err(e) => {
            error!(
                "sys_write: fd.write failed fd={} len={} err={}",
                fd_target, write_buffer.len(), e
            );
            -1
        }
    }
}
///sysread调用 traphandler栈顶
/// 使用文件描述符进行读取
pub fn sys_read(fd_target: usize, source_buffer: usize, buffer_len: usize) -> isize {
    // 获取当前任务的页表进行地址转换
    let user_satp = TASK_MANAER.get_current_stap();
    let mut buffer = PageTable::get_mut_slice_from_satp(user_satp, buffer_len, VirAddr(source_buffer));
    
    // 计算总缓冲区大小
    let total_len: usize = buffer.iter().map(|slic| slic.len()).sum();
    let mut read_buffer = vec![0u8; total_len];

    let fd = match TASK_MANAER.get_current_fd(fd_target) {
        Some(Some(fd)) => fd,
        _ => {
            warn!("sys_read: invalid fd={} len={}", fd_target, buffer_len);
            return -1;
        }
    };

    let read_len = match fd.read(&mut read_buffer) {
        Ok(len) => len,
        Err(e) => {
            error!("sys_read: fd.read failed fd={} len={} err={}", fd_target, buffer_len, e);
            return -1;
        }
    };

    let mut offset = 0usize;
    for slice in buffer.iter_mut() {
        if offset >= read_len {
            break;
        }
        let n = core::cmp::min(slice.len(), read_len - offset);
        slice[..n].copy_from_slice(&read_buffer[offset..offset + n]);
        offset += n;
    }

    read_len as isize
}


///exit系统调用，一般main程序return后在这里处理退出码 任务调度型返回-1
///注意：这个函数永不返回！要么切换到其他任务，要么关机
pub fn sys_exit(exit_code:usize)->isize{
    // 若把 init 标记为 Zombie，会导致系统只剩 Zombie/无 Ready 任务，从而调度器报错。
    let current_pid = {
        let inner = TASK_MANAER.task_que_inner.lock();
        if inner.task_queen.is_empty() {
            drop(inner);
            0
        } else {
            let current = inner.current;
            let pid = inner.task_queen[current].lock().pid.0;
            drop(inner);
            pid
        }
    };
    if current_pid == INIT_PID {
        warn!("Init exiting (pid={}), shutting down", current_pid);
        println!("Bye");
        shutdown();
    }

    // Linux 语义：exit 后任务进入 Zombie，保留 pid/exit_code，等待父进程 wait() 回收(reap)。
    // 父进程退出时，其子进程会被过继给 init(pid=1)。
    if exit_code == 0 {
        warn!("Program Exit Normaly With Code:{}", exit_code);
    } else {
        warn!("Program Exit With Code:{}", exit_code);
    }
    TASK_MANAER.reparent_current_children_to_init();
    TASK_MANAER.mark_current_zombie(exit_code as isize);
    // 进入 Zombie 后必须立刻让出 CPU
    TASK_MANAER.suspend_and_run_task();
    -1
}

/// wait 系统调用：等待任意子进程结束。
///
/// 返回：
/// - 成功：返回已回收(reap)的 Zombie 子进程 pid
/// - 失败：-1（无子进程）
pub fn sys_wait(exit_code_ptr: usize) -> isize {
    loop {
        let children = {
            let inner = TASK_MANAER.task_que_inner.lock();
            if inner.task_queen.is_empty() {
                drop(inner);
                return -1;
            }
            let current = inner.current;
            let current_task = inner.task_queen[current].clone();
            drop(inner);
            let t = current_task.lock();
            t.childrens.clone()
        };

        if children.is_empty() {
            debug!("sys_wait: no children");
            return -1;
        }

        // 寻找任意 Zombie 子进程
        for child in children.iter() {
            let pid = { child.lock().pid.0 };
            let status = { child.lock().task_statut.clone() };
            if matches!(status, TaskStatus::Zombie) {
                debug!("sys_wait: found zombie child pid={}", pid);
                let exit_code = match TASK_MANAER.reap_zombie_child(pid) {
                    Some(code) => code,
                    None => {
                        debug!("sys_wait: reap failed pid={}", pid);
                        return -1;
                    }
                };

                if exit_code_ptr != 0 {
                    let user_satp = TASK_MANAER.get_current_stap();
                    let mut slices = PageTable::get_mut_slice_from_satp(
                        user_satp,
                        size_of::<isize>(),
                        VirAddr(exit_code_ptr),
                    );
                    if slices.is_empty() {
                        return -1;
                    }
                    let bytes = exit_code.to_le_bytes();
                    let mut written = 0usize;
                    for s in slices.iter_mut() {
                        let n = core::cmp::min(s.len(), bytes.len().saturating_sub(written));
                        if n == 0 {
                            break;
                        }
                        s[..n].copy_from_slice(&bytes[written..written + n]);
                        written += n;
                    }
                    if written != bytes.len() {
                        return -1;
                    }
                }

                debug!("sys_wait: reaped child pid={} exit_code={}", pid, exit_code);
                return pid as isize;
            }
        }

        // 没有 Zombie，阻塞等待（简化：yield 让出 CPU，等待子进程退出）
        TASK_MANAER.suspend_and_run_task();
    }
}

///主动放弃cpu 任务调度型返回-1 
pub fn sys_yield()->isize{
   TASK_MANAER.suspend_and_run_task();
   -1
}


