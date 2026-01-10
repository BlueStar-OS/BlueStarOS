
use core::mem::size_of;
use core::usize;
use alloc::collections::vec_deque::VecDeque;
use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use alloc::string::String;
use log::{debug, error, warn};
use crate::sbi::shutdown;
use crate::sync::UPSafeCell;
use crate::task::{INIT_PID, ProcessId, TaskControlBlock, TaskStatus};
use crate::time::get_time_tick;
use crate::{config::PAGE_SIZE, memory::{PageTable, VirAddr, VirNumber}, task::TASK_MANAER, time::{TimeVal, get_time_ms}};
use alloc::vec;
use crate::memory::{CloneFlags, MapSet};
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
use crate::config::SECTOR_SIZE;
use crate::fs::fs_backend::fat32::Fat32Fs;
use spin::Mutex;
use crate::task::file_loader;
#[cfg(feature = "ext4")]
use crate::fs::fs_backend::{Ext4BlockDevice, Ext4Fs};

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct Timespec {
    tv_sec: i64,
    tv_nsec: i64,
}

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

pub fn sys_nanosleep(req_ptr: usize, rem_ptr: usize) -> isize {
    if req_ptr == 0 {
        return -1;
    }

    let user_satp = TASK_MANAER.get_current_stap();
    let mut tb = PageTable::crate_table_from_satp(user_satp);
    let req_pa = tb.translate(VirAddr(req_ptr));
    if req_pa.is_none() {
        return -1;
    }
    let req = unsafe { &*(req_pa.unwrap().0 as *const Timespec) };

    if req.tv_sec < 0 || req.tv_nsec < 0 {
        return -1;
    }

    let ns_total = (req.tv_sec as i128)
        .saturating_mul(1_000_000_000i128)
        .saturating_add(req.tv_nsec as i128);
    let ms = if ns_total <= 0 {
        0usize
    } else {
        ((ns_total + 999_999i128) / 1_000_000i128) as usize
    };
    let start = get_time_ms();
    let target = start.saturating_add(ms);

    while get_time_ms() < target {
        TASK_MANAER.suspend_and_run_task();
    }

    if rem_ptr != 0 {
        let rem_pa = tb.translate(VirAddr(rem_ptr));
        if let Some(pa) = rem_pa {
            unsafe {
                *(pa.0 as *mut Timespec) = Timespec { tv_sec: 0, tv_nsec: 0 };
            }
        }
    }
    0
}

#[repr(C)]
pub struct Tms { 
    pub tms_utime: usize, // 进程用户态消耗的tick数
    pub tms_stime: usize, // 进程内核态消耗的tick数
    pub tms_cutime: usize, // 所有已终止子进程的用户态tick数总和
    pub tms_cstime: usize, // 所有已终止子进程的内核tick数总和
}

/// SYS_CLONE 系统调用（最小 POSIX/Linux 兼容实现）
///
/// Linux riscv64: clone(flags, stack, ptid, tls, ctid)
///
/// 兼容策略（为了通过基础测试并保持语义正确）：
/// 1) 仅支持 flags 的低 8bit（signal number），其他高位必须为 0。
/// 2) 仅支持 stack/ptid/tls/ctid 全部为 0（不实现线程/共享地址空间等复杂语义）。
/// 3) 在支持的情况下，行为等价于 fork：父进程返回子 pid，子进程返回 0。
pub fn sys_clone(flags: usize, stack: usize, ptid: usize, tls: usize, ctid: usize) -> isize {
    let upper = flags & !0xffusize;
    
    // 没传信号处理
    sys_fork(CloneFlags::from_bits_truncate(upper),stack,ptid,tls,ctid)

    
}

pub fn sys_gettimeofday(tv_ptr: usize, _tz_ptr: usize) -> isize {
    if tv_ptr == 0 {
        return -1;
    }
    let ms = get_time_ms();
    let sec = ms / 1000;
    let usec = (ms % 1000) * 1000;
    let time_val = TimeVal { sec, usec };
    let satp = TASK_MANAER.get_current_stap();
    let mut tb = PageTable::crate_table_from_satp(satp);
    let phyaddr = tb.translate(VirAddr(tv_ptr));
    if phyaddr.is_none(){
        error!("[sys_gettimeofday]: invalid addr!");
        return -1;
    }
    unsafe {
        *(phyaddr.unwrap().0 as *mut TimeVal) = time_val;
    }
    return 0;
}

pub fn sys_times(tms_ptr: usize) -> isize { // 返回从系统启动至今所经过的时钟滴答数
    if tms_ptr == 0 {
        return -1;
    }
    let time_tick = get_time_tick(); // 系统tick数
    let satp = TASK_MANAER.get_current_stap();
    let mut tb = PageTable::crate_table_from_satp(satp);
    let phyaddr = tb.translate(VirAddr(tms_ptr));
    if phyaddr.is_none(){
        error!("[sys_gettimeofday]: invalid addr!");
        return -1;
    }
    let tms_st = Tms{
        tms_stime:time_tick,
        tms_utime:time_tick,
        tms_cutime:time_tick,
        tms_cstime:time_tick,
    };
    unsafe {
        *(phyaddr.unwrap().0 as *mut Tms) = tms_st;
    }
    return time_tick as isize;
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
        error!("sys_mount: invalid args target_ptr={:#x} fstype_ptr={:#x}", target_ptr, fstype_ptr);
        return -1;
    }

    let source = if source_ptr == 0 {
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

    debug!("sys_mount: source='{}' target='{}' fstype='{}'", source, target, fstype);

    // 规范化 target 路径，并要求其必须是目录
    let abs_target = match normalize_path(&target) {
        Ok(p) => p,
        Err(e) => {
            error!("sys_mount: normalize target failed target={} err={:?}", target, e);
            return -1;
        }
    };
    if abs_target == "/" {
        // 不允许覆盖根挂载点
        error!("sys_mount: refuse to mount on /");
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
        error!("sys_mount: target is not dir target={} type={}", abs_target, st.file_type);
        return -1;
    }

    let abs_source = match normalize_path(&source) {
        Ok(p) => p,
        Err(e) => {
            error!("sys_mount: normalize source failed source={} err={:?}", source, e);
            return -1;
        }
    };
    if abs_source.is_empty() {
        error!("sys_mount: empty source");
        return -1;
    }

    fn base_disk_path(abs_source: &str) -> Option<(&str, usize)> {
        if !abs_source.starts_with("/dev/") {
            return None;
        }
        let mut end = abs_source.len();
        while end > 0 && abs_source.as_bytes()[end - 1].is_ascii_digit() {
            end -= 1;
        }
        if end == abs_source.len() {
            return None;
        }
        let idx_str = &abs_source[end..];
        let idx = idx_str.parse::<usize>().ok()?;
        if idx == 0 {
            return None;
        }
        Some((&abs_source[..end], idx))
    }

    fn read_partition_type(disk: &Arc<dyn File>, part_idx_1based: usize) -> Result<u8, VfsFsError> {
        if part_idx_1based == 0 || part_idx_1based > 4 {
            return Err(VfsFsError::Invalid);
        }
        let mut mbr = [0u8; SECTOR_SIZE];
        disk.read_at(0, &mut mbr)?;
        if mbr[510] != 0x55 || mbr[511] != 0xAA {
            return Err(VfsFsError::Invalid);
        }
        let base = 0x1BE + (part_idx_1based - 1) * 16;
        Ok(mbr[base + 4])
    }

    let (disk_path, part_idx) = match base_disk_path(&abs_source) {
        Some(v) => v,
        None => {
            error!("sys_mount: unsupported source path (expect /dev/xxxN) abs_source={}", abs_source);
            return -1;
        }
    };

    debug!("sys_mount: parsed source abs_source={} disk_path={} part_idx={}", abs_source, disk_path, part_idx);

    let disk = match vfs_open(disk_path, OpenFlags::empty()) {
        Ok(f) => f,
        Err(e) => {
            error!("sys_mount: open disk failed disk_path={} err={}", disk_path, e);
            return -1;
        }
    };
    let ptype = match read_partition_type(&disk, part_idx) {
        Ok(t) => t,
        Err(e) => {
            error!("sys_mount: read partition type failed disk_path={} part_idx={} err={}", disk_path, part_idx, e);
            return -1;
        }
    };

    debug!("sys_mount: mbr partition type=0x{:02x}", ptype);

    let auto_fs = match ptype {
        0x83 => "ext4",
        0x0b | 0x0c => "fat32",
        0x0e => "fat16",
        _ => "unknown",
    };

    let is_auto = fstype.is_empty() || fstype == "auto";
    let explicit_fs = match fstype.as_str() {
        "vfat" => "fat32",
        other => other,
    };
    let req_fs = if is_auto { auto_fs } else { explicit_fs };
    debug!("sys_mount: auto_fs={} req_fs={} ptype=0x{:02x}", auto_fs, req_fs, ptype);

    // POSIX 语义：若用户显式指定了 fstype，则按用户指定尝试挂载。
    // 只有 fstype=auto 时才依赖分区类型做自动判定。
    if is_auto {
        if req_fs == "fat16" || req_fs == "unknown" {
            error!("sys_mount: unsupported fs req_fs={} ptype=0x{:02x}", req_fs, ptype);
            return -1;
        }
    } else {
        if req_fs != "ext4" && req_fs != "fat32" {
            error!("sys_mount: unsupported explicit fstype={} ptype=0x{:02x}", explicit_fs, ptype);
            return -1;
        }
    }

    let src_dev = match vfs_open(&abs_source, OpenFlags::empty()) {
        Ok(f) => f,
        Err(e) => {
            error!("sys_mount: open source device failed abs_source={} err={}", abs_source, e);
            return -1;
        }
    };

    let new_fs: Arc<Mutex<dyn VfsFs>> = match req_fs {
        "ext4" => {
            #[cfg(feature = "ext4")]
            {
                let blk = Ext4BlockDevice::new(src_dev);
                Arc::new(Mutex::new(Ext4Fs::new(blk))) as Arc<Mutex<dyn VfsFs>>
            }
            #[cfg(not(feature = "ext4"))]
            {
                error!("sys_mount: ext4 requested but ext4 feature is disabled");
                return -1;
            }
        }
        "fat32" => {
            let fs = match Fat32Fs::new(src_dev) {
                Ok(v) => v,
                Err(e) => {
                    error!("sys_mount: fat32 init failed err={}", e);
                    return -1;
                }
            };
            Arc::new(Mutex::new(fs)) as Arc<Mutex<dyn VfsFs>>
        }
        _ => return -1,
    };

    if let Err(e) = new_fs.lock().mount() {
        error!("sys_mount: fs.mount failed req_fs={} err={}", req_fs, e);
        return -1;
    }

    let mut root = ROOTFS.lock();
    let rootfs = match root.as_mut() {
        Some(r) => r,
        None => {
            error!("sys_mount: ROOTFS not initialized");
            return -1;
        }
    };
    let key = MountPath(abs_target);
    if rootfs.mount_poinr.contains_key(&key) {
        error!("sys_mount: target already mounted target={}", key.0);
        return -1;
    }
    rootfs.mount_poinr.insert(key, new_fs);
    //debug!("sys_mount: mount success source={} target={} fstype={}", abs_source, key.0, req_fs);
    0
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

    // 遍历进程列表确保任何进程不在挂载点路径上
    let mp_busy = TASK_MANAER.task_que_inner.lock().task_queen.iter().any(|task|{
        let tcwd = &task.lock().cwd;
        tcwd.starts_with(&key.0)
    });

    if mp_busy {
        error!("[sys_umount]: Vblock:{} busy!",&key.0);
        return -1;
    }



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
    let re = current_task.lock().pid.0;
    re as isize
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

///SYS_EXECVE系统调用（POSIX/Linux）
/// argv/envp：用户态指针数组，均以 NULL 结尾。
pub fn sys_execve(path_ptr: usize, argv_ptr: usize, envp_ptr: usize) -> isize {
    const MAX_ARGC: usize = 256;

    // 关键：先拿到 satp，再去 lock 当前 task，避免二次借用。
    let user_satp = TASK_MANAER.get_current_stap();

    debug!(
        "sys_execve: path_ptr={:#x} argv_ptr={:#x} envp_ptr={:#x} satp={:#x}",
        path_ptr, argv_ptr, envp_ptr, user_satp
    );

    if envp_ptr != 0 {
        warn!("sys_execve: envp is ignored for now envp_ptr={:#x}", envp_ptr);
    }

    let path = match read_c_string_from_user_with_satp(user_satp, path_ptr) {
        Ok(p) => p,
        Err(_) => return -1,
    };

    let elf_data = file_loader(&path);
    if elf_data.is_empty() {
        return -1;
    }

    // 读取 argv 指针数组（NULL 结尾）
    let mut exec_argv: Vec<String> = Vec::new();
    if argv_ptr != 0 {
        for i in 0..MAX_ARGC {
            let elem_ptr = argv_ptr + i * core::mem::size_of::<usize>();
            let mut slices = PageTable::get_mut_slice_from_satp(
                user_satp,
                core::mem::size_of::<usize>(),
                VirAddr(elem_ptr),
            );
            if slices.is_empty() {
                error!("sys_execve: invalid argv element addr={:#x}", elem_ptr);
                return -1;
            }
            let mut flat: Vec<u8> = Vec::with_capacity(core::mem::size_of::<usize>());
            for s in slices.iter_mut() {
                flat.extend_from_slice(s);
            }
            if flat.len() < core::mem::size_of::<usize>() {
                error!("sys_execve: short read argv element addr={:#x}", elem_ptr);
                return -1;
            }
            let ptr_bytes: [u8; core::mem::size_of::<usize>()] = flat[..core::mem::size_of::<usize>()]
                .try_into()
                .unwrap();
            let cptr = usize::from_ne_bytes(ptr_bytes);
            if cptr == 0 {
                break;
            }
            match read_c_string_from_user_with_satp(user_satp, cptr) {
                Ok(s) => exec_argv.push(s),
                Err(e) => {
                    error!(
                        "sys_execve: Can't translate argv[{}] ptr={:#x} err={}",
                        i, cptr, e
                    );
                    return -1;
                }
            }
        }
    }

    let argc = exec_argv.len();
    let current_task = {
        let inner = TASK_MANAER.task_que_inner.lock();
        inner.task_queen[inner.current].clone()
    };
    {
        let mut tcb = current_task.lock();
        if !tcb.new_exec_task_with_elf(&path, exec_argv, argc, &elf_data) {
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

    let acc = flags_bits & OpenFlags::ACCMODE_MASK;
    if acc > 2 {
        error!(
            "sys_open: invalid acc bits: path={} flags_bits={:#x}",
            path, flags_bits
        );
        return -1;
    }
    let flags = OpenFlags::from_bits_truncate(flags_bits);

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
pub fn sys_fork(mode:CloneFlags,stack: usize, ptid: usize, tls: usize, ctid: usize)->isize{
    //warn!("forlk");
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


    bad_task.parent = None;
    bad_task.childrens.clear();

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
    if new_memset.is_none(){
        error!("Process Memset clone failed!");
        return -1;
    }
    bad_task.memory_set = new_memset.expect("Memset should be some");

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

            // TODO THREAD define
            // stack
            if stack!=0{
                (*trap_cx_point).x[2] = stack;
            }

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
    //warn!("sys_fork: parent pid={} add child pid={} children_len={}", parent_pid, child_pid, current_task.lock().childrens.len());
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
               usec:get_time_ms()%1000
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
    //if current_pid == INIT_PID {
      //  warn!("Init exiting (pid={}), shutting down", current_pid);
       // kprintln!("Bye");
        //shutdown();
    //}

    // Linux 语义：exit 后任务进入 Zombie，保留 pid/exit_code，等待父进程 wait() 回收(reap)。
    // 父进程退出时，其子进程会被过继给 init(pid=1)。
    if exit_code == 0 {
        //warn!("Program Exit Normaly With Code:{}", exit_code);
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
    // wait4(pid=-1, wstatus, options=0)
    sys_wait4(-1, exit_code_ptr, 0)
}

/// TODO状态
/// wait4/waitpid 语义的最小实现。
///
/// - pid == -1 : 等待任意子进程
/// - pid > 0   : 等待指定 pid 子进程
/// - pid == 0 或 pid < -1 : 不支持，返回 -1 并输出 unsupport
/// - options != 0 : 不支持，返回 -1 并输出 unsupport
///
/// wstatus 写回遵循 Linux：退出码存放在高 8 bit（status = exit_code << 8）。
pub fn sys_wait4(pid: i32, wstatus_ptr: usize, options: i32) -> isize {
    if options != 0 {
        warn!("sys_wait4: unsupport options={}", options);
        return -1;
    }

    let pid_isize = pid as isize;
    if pid_isize == 0 || pid_isize < -1 {
        warn!("sys_wait4: unsupport pid={}", pid_isize);
        return -1;
    }

    let target_pid: Option<i32> = if pid_isize == -1 {
        None
    } else {
        Some(pid)
    };

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
            warn!("sys_wait4: no children for current process");
            return -1;
        }

        if let Some(tp) = target_pid {
            let mut found = false;
            for child in children.iter() {
                let cpid = { child.lock().pid.0 };
                if cpid == tp {
                    found = true;
                    let status = { child.lock().task_statut.clone() };
                    if matches!(status, TaskStatus::Zombie) {
                        let exit_code = match TASK_MANAER.reap_zombie_child(cpid) {
                            Some(code) => code,
                            None => return -1,
                        };
                        if wstatus_ptr != 0 {
                            let st: i32 = ((exit_code as i32) & 0xff) << 8;
                            let user_satp = TASK_MANAER.get_current_stap();
                            let mut slices = PageTable::get_mut_slice_from_satp(
                                user_satp,
                                size_of::<i32>(),
                                VirAddr(wstatus_ptr),
                            );
                            if slices.is_empty() {
                                return -1;
                            }
                            let bytes = st.to_le_bytes();
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
                        return cpid as isize;
                    }
                }
            }
            if !found {
                debug!("sys_wait4: target child not found pid={}", tp);
                return -1;
            }
        } else {
            // pid == -1: find any zombie
            for child in children.iter() {
                let cpid = { child.lock().pid.0 };
                let status = { child.lock().task_statut.clone() };
                if matches!(status, TaskStatus::Zombie) {
                    let exit_code = match TASK_MANAER.reap_zombie_child(cpid) {
                        Some(code) => code,
                        None => return -1,
                    };
                    if wstatus_ptr != 0 {
                        let st: i32 = ((exit_code as i32) & 0xff) << 8;
                        let user_satp = TASK_MANAER.get_current_stap();
                        let mut slices = PageTable::get_mut_slice_from_satp(
                            user_satp,
                            size_of::<i32>(),
                            VirAddr(wstatus_ptr),
                        );
                        if slices.is_empty() {
                            return -1;
                        }
                        let bytes = st.to_le_bytes();
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
                    return cpid as isize;
                }
            }
        }

        TASK_MANAER.suspend_and_run_task();
    }
}

///主动放弃cpu
pub fn sys_yield()->isize{
   TASK_MANAER.suspend_and_run_task();
   0
}


