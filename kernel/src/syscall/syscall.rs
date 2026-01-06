
use core::mem::size_of;
use alloc::collections::vec_deque::VecDeque;
use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use alloc::string::String;
use log::{debug, error, warn};
use crate::sbi::shutdown;
use crate::sync::UPSafeCell;
use crate::task::{INIT_PID, ProcessId, TaskControlBlock, TaskStatus};
use crate::{config::PAGE_SIZE, memory::{PageTable, VirAddr, VirNumber}, task::TASK_MANAER, time::{TimeVal, get_time_ms}};
use alloc::vec;
use crate::memory::MapSet;
use crate::fs::vfs::{self, VfsFsError, normalize_path};
use crate::fs::vfs::{vfs_fstat_kstat, vfs_getdents64, vfs_mkdir, vfs_open, vfs_stat, vfs_unlink, KStat, OpenFlags, VfsStat, VFS_DT_DIR};
use crate::fs::vfs::File;
use crate::fs::component::pipe::pipe::{make_pipe, PipeHandle};
use crate::trap::TrapContext;
use crate::TRAP_CONTEXT_ADDR;
use crate::task::ProcessId_ALLOCTOR;
use crate::task::TaskContext;
use crate::alloc::string::ToString;
use alloc::format;
use crate::memory::PTEFlags;
use crate::fs::vfs::{ROOTFS, MountPath, VfsFs};

#[cfg(feature = "ext4")]
use crate::driver::VirtBlk;
#[cfg(feature = "ext4")]
use crate::fs::fs_backend::{Ext4BlockDevice, Ext4Fs};
#[cfg(feature = "ext4")]
use spin::Mutex;

///SYS_UNAME系统调用
/// 传入新旧文件name
/// utsname结构体
const utname_field_len:usize = 65;//byte
#[repr(C)]
#[derive(Debug)]
struct utsname{
    sysname:[u8;utname_field_len], //当前操作系统名
    nodename:[u8;utname_field_len], //主机名hostname
    release:[u8;utname_field_len], //当前发布级别
    version:[u8;utname_field_len], //内核版本字符串
    machine:[u8;utname_field_len], //当前硬件结构
    domainname:[u8;utname_field_len], //NIS DOMAIN name
}

/// POSIX/Linux: mount(source, target, filesystemtype, mountflags, data)
///
/// 中文说明（当前内核的最小实现/简化点）：
/// 1) 只支持通过 `target` 路径创建挂载点（必须是已存在目录，且不能是 `/`）。
/// 2) `source` / `mountflags` / `data` 目前不参与实际行为（仅做参数读取/基本校验）。
/// 3) `filesystemtype` 目前仅支持 "ext4"（在 feature=ext4 下生效）。
/// 4) 返回值遵循 POSIX：成功返回 0，失败返回 -1。
pub fn sys_mount(source_ptr: usize, target_ptr: usize, fstype_ptr: usize, _flags: usize, _data_ptr: usize) -> isize {
    if target_ptr == 0 || fstype_ptr == 0 {
        return -1;
    }

    let _source = if source_ptr == 0 {
        String::new()
    } else {
        match read_c_string_from_user(source_ptr) {
            Ok(s) => s,
            Err(e) => {
                error!("sys_mount: invalid source ptr={:#x} err={}", source_ptr, e);
                return -1;
            }
        }
    };

    let target = match read_c_string_from_user(target_ptr) {
        Ok(s) => s,
        Err(e) => {
            error!("sys_mount: invalid target ptr={:#x} err={}", target_ptr, e);
            return -1;
        }
    };

    let fstype = match read_c_string_from_user(fstype_ptr) {
        Ok(s) => s,
        Err(e) => {
            error!("sys_mount: invalid fstype ptr={:#x} err={}", fstype_ptr, e);
            return -1;
        }
    };

    // 规范化 target 路径，并要求其必须是目录
    let abs_target = match normalize_path(&target) {
        Ok(p) => p,
        Err(_) => return -1,
    };
    if abs_target == "/" {
        // 不允许覆盖根挂载点
        return -1;
    }
    let st = match vfs_stat(&abs_target) {
        Ok(s) => s,
        Err(e) => {
            error!("sys_mount: target stat failed target={} err={}", abs_target, e);
            return -1;
        }
    };
    if st.file_type != VFS_DT_DIR {
        return -1;
    }

   return 0;
    #[cfg(not(feature = "ext4"))]
    {
        let _ = fstype;
        -1
    }
}

/// POSIX/Linux: umount2(target, flags)
///
/// 中文说明（当前内核的最小实现/简化点）：
/// 1) 仅支持按 `target` 卸载挂载点；不支持 lazy/unlink 等 flags 语义（flags 暂时忽略）。
/// 2) 不允许卸载根挂载点 `/`。
/// 3) 返回值遵循 POSIX：成功返回 0，失败返回 -1。
pub fn sys_umount2(target_ptr: usize, _flags: usize) -> isize {
    if target_ptr == 0 {
        return -1;
    }
    let target = match read_c_string_from_user(target_ptr) {
        Ok(s) => s,
        Err(e) => {
            error!("sys_umount2: invalid target ptr={:#x} err={}", target_ptr, e);
            return -1;
        }
    };
    let abs_target = match normalize_path(&target) {
        Ok(p) => p,
        Err(_) => return -1,
    };
    if abs_target == "/" {
        return -1;
    }

    let mut root = ROOTFS.lock();
    let rootfs = match root.as_mut() {
        Some(r) => r,
        None => return -1,
    };

    let key = MountPath(abs_target);
    let Some(fs) = rootfs.mount_poinr.remove(&key) else {
        return -1;
    };

    if let Err(e) = fs.lock().umount() {
        error!("sys_umount2: fs.umount failed err={}", e);
        // best-effort: keep entry removed to avoid inconsistent resolution
        return -1;
    }
    0
}
impl utsname {
    pub fn new()->Self{
        Self { sysname: [0;utname_field_len], nodename: [0;utname_field_len], release: [0;utname_field_len], version: [0;utname_field_len],
             machine: [0;utname_field_len], domainname: [0;utname_field_len] }
    }
}
///buf:&mut utsname as *mut _ as usize
pub fn sys_uname(buf:usize)->isize{
    if buf == 0 {
        return -1;
    }

    fn fill_field(dst: &mut [u8; utname_field_len], s: &str) {
        dst.fill(0);
        let bytes = s.as_bytes();
        let n = core::cmp::min(bytes.len(), utname_field_len - 1);//give \0 one byte
        dst[..n].copy_from_slice(&bytes[..n]);
        dst[n] = 0;
    }

    fn user_range_writable(satp: usize, start: usize, len: usize) -> bool {
        if len == 0 {
            return true;
        }
        let mut pt = PageTable::crate_table_from_satp(satp);
        let start_addr = VirAddr(start);
        let end_addr = VirAddr(start.saturating_add(len));
        let mut addr = start_addr;
        while addr < end_addr {
            let vpn = addr.floor_down();
            let Some(pte) = pt.find_pte_vpn(vpn) else {
                return false;
            };
            if !pte.is_valid() {
                return false;
            }
            let flags = pte.flags();
            if !flags.contains(PTEFlags::U) || !flags.contains(PTEFlags::W) {
                return false;
            }
            let next_page: VirAddr = VirNumber(vpn.0 + 1).into();
            addr = next_page;
        }
        true
    }

    fn copy_to_user(satp: usize, dst: usize, src: &[u8]) -> bool {
        let mut pt = PageTable::crate_table_from_satp(satp);
        for (i, b) in src.iter().enumerate() {
            let vaddr = VirAddr(dst.saturating_add(i));
            let Some(paddr) = pt.translate(vaddr) else {
                return false;
            };
            unsafe {
                *(paddr.0 as *mut u8) = *b;
            }
        }
        true
    }

    let user_satp = TASK_MANAER.get_current_stap();
    let total_len = core::mem::size_of::<utsname>();
    if !user_range_writable(user_satp, buf, total_len) {
        return -1;
    }

    let mut u = utsname::new();
    fill_field(&mut u.sysname, "Linux");
    fill_field(&mut u.nodename, "BlueStarOS");
    fill_field(&mut u.release, "0.1.0");
    fill_field(&mut u.version, "#1");
    fill_field(&mut u.machine, "riscv64");
    fill_field(&mut u.domainname, "(none)");

    let bytes: &[u8] = unsafe {
        core::slice::from_raw_parts((&u as *const utsname) as *const u8, total_len)
    };
    if !copy_to_user(user_satp, buf, bytes) {
        return -1;
    }
    0
}

///SYS_DUP2系统调用
/// 返回一个符合最小fd的结果
/// 传入需要复制的fd
pub fn sys_dup2(old_fd:i32,new_fd:i32) ->isize{

    if old_fd < 0 || new_fd < 0 {
        return -1;
    }

    let current_task = {
        let inner = TASK_MANAER.task_que_inner.lock();
        inner.task_queen[inner.current].clone()
    };

    let mut tcb = current_task.lock();

    let old_idx = old_fd as usize;
    if old_idx >= tcb.file_descriptor.len() {
        return -1;
    }
    let Some(source_fd) = tcb.file_descriptor[old_idx].clone() else {
        return -1;
    };

    if old_fd == new_fd {
        return new_fd as isize;
    }

    let new_idx = new_fd as usize;
    if new_idx >= tcb.file_descriptor.len() {
        tcb.file_descriptor.resize_with(new_idx + 1, || None);
    }

    // close(newfd) if it is open
    tcb.file_descriptor[new_idx] = None;
    tcb.file_descriptor[new_idx] = Some(source_fd);
    new_fd as isize
}

///SYS_DUP系统调用
/// 返回一个符合最小fd的结果
/// 传入需要复制的fd
pub fn sys_dup(old_fd:i32) ->isize{
    let current_task = {
        let inner = TASK_MANAER.task_que_inner.lock();
        inner.task_queen[inner.current].clone()
    };

    let mut tcb = current_task.lock();

    if old_fd < 0 {
        return -1;
    }
    let old_idx = old_fd as usize;
    if old_idx >= tcb.file_descriptor.len() {
        return -1;
    }
    let Some(source_fd) = tcb.file_descriptor[old_idx].clone() else {
        return -1;
    };

    if let Some((idx, _)) = tcb
        .file_descriptor
        .iter()
        .enumerate()
        .find(|(_, slot)| slot.is_none())
    {
        tcb.file_descriptor[idx] = Some(source_fd);
        return idx as isize;
    }

    // No empty slot: grow fd table.
    let idx = tcb.file_descriptor.len();
    tcb.file_descriptor.push(Some(source_fd));
    idx as isize
}

pub fn sys_getpid() -> isize {
    let current_task = {
        let inner = TASK_MANAER.task_que_inner.lock();
        inner.task_queen[inner.current].clone()
    };
    current_task.lock().pid.0 as isize
}

pub fn sys_getppid() -> isize {
    let current_task = {
        let inner = TASK_MANAER.task_que_inner.lock();
        inner.task_queen[inner.current].clone()
    };
    let tcb = current_task.lock();
    if let Some(parent) = tcb.parent.as_ref().and_then(|w| w.upgrade()) {
        parent.lock().pid.0 as isize
    } else {
        0
    }
}

///SYS_BRK系统调用 
/// brk->堆顶, new_brk可不对齐，由用户库处理
/// 传入0返回当前brk地址（用户空间），其它地址->尝试brk，失败的话返回原来的brk，成功返回新的brk
pub fn sys_brk(new_brk:VirAddr)->isize{ 
    let new_brkaddr = new_brk.0;

    // 先取出当前 task 的 Arc，避免持有 task queue 的锁期间再 lock task。
    let current_task = {
        let inner = TASK_MANAER.task_que_inner.lock();
        inner.task_queen[inner.current].clone()
    };

    let mut tcb = current_task.lock();
    let old_brk = tcb.memory_set.brk.0;

    // Linux 语义：brk(0) 只查询当前 break。
    if new_brkaddr == 0 {
        return old_brk as isize;
    }

    // shrink：先只更新 brk，不回收映射（最小实现，优先兼容测试）。
    if new_brkaddr <= old_brk {
        tcb.memory_set.brk = VirAddr(new_brkaddr);
        return new_brkaddr as isize;
    }

    // expand：需要把 [old_brk, new_brk) 涉及到的新页映射出来。
    // 已经映射的旧页不需要重复映射：从包含 old_brk 的页的下一页开始。
    let mut start_vpn: VirNumber = VirAddr(old_brk).floor_down();
    start_vpn.step();
    let end_vpn: VirNumber = VirAddr(new_brkaddr - 1).floor_down();

    if start_vpn.0 <= end_vpn.0 {
        // 注意：add_area 会检查区间是否与现有 MapArea 重叠，
        // 所以这里从 floor_up(old_brk) 开始，避免覆盖旧页。
        tcb.memory_set.add_area(
            crate::memory::VirNumRange(start_vpn, end_vpn),
            crate::memory::MapType::Maped,
            crate::memory::MapAreaFlags::R | crate::memory::MapAreaFlags::W | crate::memory::MapAreaFlags::U,
            None,
            None,
        );
    }

    tcb.memory_set.brk = VirAddr(new_brkaddr);
    new_brkaddr as isize
}

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
    let read_fd: Arc<dyn File> = Arc::new(PipeHandle::new(read_end));
    let write_fd: Arc<dyn File> = Arc::new(PipeHandle::new(write_end));

    let rfd:i32 = TASK_MANAER.alloc_fd_for_current(read_fd);
    if rfd < 0 {
        return -1;
    }
    let wfd:i32 = TASK_MANAER.alloc_fd_for_current(write_fd);
    if wfd < 0 {
        return -1;
    }

    let user_satp = TASK_MANAER.get_current_stap();
    let mut slices = PageTable::get_mut_slice_from_satp(
        user_satp,
        core::mem::size_of::<usize>() * 2,
        VirAddr(fds_ptr),
    );

    let mut tmp: [u8; core::mem::size_of::<i32>() * 2] = [0u8; core::mem::size_of::<i32>() * 2];
    tmp[..core::mem::size_of::<i32>()].copy_from_slice(&rfd.to_ne_bytes());
    tmp[core::mem::size_of::<i32>()..].copy_from_slice(&wfd.to_ne_bytes());

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

pub fn sys_chdir(path_ptr: usize) -> isize {
    let path = match read_c_string_from_user(path_ptr) {
        Ok(p) => p,
        Err(e) => {
            error!("sys_chdir: invalid user path ptr={:#x}, err={}", path_ptr, e);
            return -1;
        }
    };

    
    let abs = match normalize_path(&path) {
        Ok(p) => p,
        Err(_) => return -1,
    };

    let st = match vfs_stat(&abs) {
        Ok(s) => s,
        Err(e) => {
            error!("sys_chdir: vfs_stat failed: path={} err={}", abs, e);
            return -1;
        }
    };
    if st.file_type != VFS_DT_DIR {
        return -1;
    }

    TASK_MANAER.set_current_cwd(abs);
    0
}

pub fn sys_getcwd(user_buf_ptr: usize, buf_len: usize) -> isize {
    if user_buf_ptr == 0 || buf_len == 0 {
        return 0;
    }

    let cwd = TASK_MANAER.get_current_cwd();
    let mut tmp: Vec<u8> = Vec::new();
    tmp.extend_from_slice(cwd.as_bytes());
    tmp.push(0);

    if tmp.len() > buf_len {
        return 0;
    }

    let user_satp = TASK_MANAER.get_current_stap();
    let mut slices = PageTable::get_mut_slice_from_satp(user_satp, tmp.len(), VirAddr(user_buf_ptr));
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
        return 0;
    }
    user_buf_ptr as isize
}

pub fn sys_mkdirat(dirfd: isize, path_ptr: usize, _mode: usize) -> isize {
    // NOTE: oscomp uses mkdir() implemented via mkdirat(AT_FDCWD,...,mode).
    // We currently ignore dirfd/mode and rely on VFS/ext4 without permission bits.
    let _ = dirfd;
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

pub fn sys_mkdir(path_ptr: usize) -> isize {
    sys_mkdirat(-100, path_ptr, 0)
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

    let kst: KStat = st.into();

    let user_satp = TASK_MANAER.get_current_stap();
    let mut slices = PageTable::get_mut_slice_from_satp(
        user_satp,
        core::mem::size_of::<KStat>(),
        VirAddr(stat_buf_ptr),
    );

    let bytes: &[u8] = unsafe {
        core::slice::from_raw_parts((&kst as *const KStat) as *const u8, core::mem::size_of::<KStat>())
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

pub fn sys_fstat(fd: usize, stat_buf_ptr: usize) -> isize {
    if stat_buf_ptr == 0 {
        error!("sys_fstat: null stat_buf_ptr fd={}", fd);
        return -1;
    }

    let file = match TASK_MANAER.get_current_fd(fd) {
        Some(Some(f)) => f,
        _ => {
            warn!("sys_fstat: invalid fd={}", fd);
            return -1;
        }
    };

    let kst: KStat = match vfs_fstat_kstat(&file) {
        Ok(s) => s,
        Err(e) => {
            error!("sys_fstat: vfs_fstat_kstat failed: fd={} err={}", fd, e);
            return -1;
        }
    };

    let user_satp = TASK_MANAER.get_current_stap();
    let mut slices = PageTable::get_mut_slice_from_satp(
        user_satp,
        core::mem::size_of::<KStat>(),
        VirAddr(stat_buf_ptr),
    );

    let bytes: &[u8] = unsafe {
        core::slice::from_raw_parts((&kst as *const KStat) as *const u8, core::mem::size_of::<KStat>())
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
        error!("sys_fstat: short copy to user: fd={} copied={} need={}", fd, off, bytes.len());
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
    let fd = TASK_MANAER.alloc_fd_for_current(opened);
    if fd < 0 {
        error!("sys_open: alloc fd failed: path={} flags_bits={:#x}", path, flags_bits);
    }
    fd as isize
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

    debug!(
        "read_c_string_from_user_with_satp: satp={:#x} path_ptr={:#x}",
        user_satp, path_ptr
    );

    // 不要一次性取 MAX_PATH_LEN 的 slice：字符串可能位于页尾，跨页会触发内核态 fault。
    // 这里逐字节翻译虚拟地址并读取，直到遇到 '\0' 或超过最大长度。
    let mut table = PageTable::crate_table_from_satp(user_satp);
    let mut data: Vec<u8> = Vec::new();

    for off in 0..MAX_PATH_LEN {
        let vaddr = VirAddr(path_ptr + off);
        let paddr = match table.translate(vaddr) {
            Some(p) => p,
            None => {
                error!(
                    "read_c_string_from_user_with_satp: translate failed: satp={:#x} path_ptr={:#x} off={} vaddr={:#x}",
                    user_satp,
                    path_ptr,
                    off,
                    vaddr.0
                );
                return Err(VfsFsError::Invalid);
            }
        };
        let b = unsafe { *(paddr.0 as *const u8) };
        if b == 0 {
            debug!(
                "read_c_string_from_user_with_satp: found NUL: len={} satp={:#x} path_ptr={:#x}",
                data.len(),
                user_satp,
                path_ptr
            );
            let s = core::str::from_utf8(&data)
                .map_err(|_| VfsFsError::Invalid)?
                .to_string();
            debug!("read_c_string_from_user_with_satp: str='{}'", s);
            return Ok(s);
        }
        data.push(b);
    }

    error!(
        "read_c_string_from_user_with_satp: no NUL within {} bytes: satp={:#x} path_ptr={:#x}",
        MAX_PATH_LEN,
        user_satp,
        path_ptr
    );
    Err(VfsFsError::Invalid)
}


///mmap系统调用
/// Linux/POSIX: mmap(addr, len, prot, flags, fd, offset)
/// 返回：成功返回映射起始地址；失败返回 -1
///
/// 参数说明（Linux riscv64 ABI，用户态用 ecall 传参）：
/// `addr`  : 映射起始虚拟地址（用户 hint）。若带 `MAP_FIXED` 则必须使用该地址。
/// `len`   : 映射长度（字节）。内核按页对齐到覆盖该区间。
/// `prot`  : 访问权限位（PROT_*）：
///           - PROT_READ=0x1
///           - PROT_WRITE=0x2
///           - PROT_EXEC=0x4
/// `flags` : 映射标志（MAP_*），至少需要指定其一：
///           - MAP_SHARED=0x01 或 MAP_PRIVATE=0x02
///           - MAP_FIXED=0x10（可选）
///           - MAP_ANONYMOUS=0x20（匿名映射）
/// `fd`    : 文件描述符。匿名映射时要求 `fd == -1`；文件映射时为有效 fd。
/// `offset`: 文件偏移（字节）。必须页对齐（offset % PAGE_SIZE == 0）。匿名映射时通常为 0。
///
/// 当前最小实现：仅支持匿名映射（`MAP_ANONYMOUS` 且 `fd == -1`），并要求 `addr != 0`。
pub fn sys_mmap(addr: usize, len: usize, prot: usize, flags: usize, fd: i32, offset: usize) -> isize {
    //warn!("enter mmap");
    let inner = TASK_MANAER.task_que_inner.lock();
    let current = inner.current;
    drop(inner);
    let fd_backing =match TASK_MANAER.get_current_fd(fd as usize){
        Some(v)=>v,
        _=>None
    };
    let inner = TASK_MANAER.task_que_inner.lock();
    let mut tcb = inner.task_queen[current].lock();
    
     
    tcb.memory_set.mmap(VirAddr(addr), len, prot, flags, fd, offset,fd_backing)
}


///unmap系统调用
/// startaddr:usize size:长度
pub fn sys_munmap(start:usize,size:usize)->isize{
    let inner=TASK_MANAER.task_que_inner.lock();
    let current=inner.current;
    drop(inner);
    let  inner=TASK_MANAER.task_que_inner.lock();
    let  memset=&mut inner.task_queen[current].lock().memory_set;
    memset.unmap_range(VirAddr(start), size)
    //inner自动销毁
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
                "sys_write: fd.write failed fd={} req_len={} copied_len={}  err={}",
                fd_target,
                buffer_len,
                write_buffer.len(),
                e
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
        kprintln!("Bye");
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


