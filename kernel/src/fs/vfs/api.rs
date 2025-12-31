//!上层通用接口
use alloc::collections::BTreeMap;
use alloc::boxed::Box;
use alloc::vec::Vec;
use alloc::{format, string::String, sync::Arc};
use spin::Mutex;
use crate::alloc::string::ToString;
use crate::fs::fs_backend::Ext4Fs;
use crate::fs::vfs::{ROOTFS, VfsFsError};
use crate::sync::UPSafeCell;
use log::{debug, error};
use lazy_static::lazy_static;

use rsext4::{
    OpenFile,
    lseek as ext4_lseek,
    mkdir as ext4_mkdir,
    mkfile as ext4_mkfile,
    mv as ext4_mv,
    open as ext4_open,
    read_at as ext4_read_at,
    read as ext4_read,
    rename as ext4_rename,
    truncate as ext4_truncate,
    write_at as ext4_write_at,
};

use rsext4::ext4_backend::dir::get_inode_with_num;
use rsext4::ext4_backend::entries::DirEntryIterator;
use rsext4::ext4_backend::file::unlink as ext4_unlink;
use rsext4::ext4_backend::loopfile::resolve_inode_block_allextend;
use rsext4::ext4_backend::config::BLOCK_SIZE;

pub trait FileDescriptorTrait: Send + Sync {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, VfsFsError>;
    fn write(&mut self, buf: &[u8]) -> Result<usize, VfsFsError>;

    fn read_at(&mut self, _offset: usize, _buf: &mut [u8]) -> Result<usize, VfsFsError> {
        Err(VfsFsError::FsInnerError)
    }

    fn write_at(&mut self, _offset: usize, _buf: &[u8]) -> Result<usize, VfsFsError> {
        Err(VfsFsError::FsInnerError)
    }

    fn lseek(&mut self, _offset: isize, _whence: usize) -> Result<usize, VfsFsError> {
        Err(VfsFsError::FsInnerError)
    }

    fn path(&self) -> Option<&str> {
        None
    }

    fn offset(&self) -> Option<u64> {
        None
    }

    fn set_offset(&mut self, _off: u64) -> Result<(), VfsFsError> {
        Err(VfsFsError::FsInnerError)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct VfsStat {
    pub inode: u32,
    pub size: u64,
    pub mode: u32,
    pub file_type: u32,
}

#[repr(C, packed)]
#[derive(Clone, Copy, Debug, Default)]
pub struct LinuxDirent64 {
    pub d_ino: u64,
    pub d_off: u64,
    pub d_reclen: u16,
    pub d_type: u8,
    // d_name starts right after this header (offset 19)
}

const VFS_DT_UNKNOWN: u32 = 0;
const VFS_DT_REG: u32 = 8;
const VFS_DT_DIR: u32 = 4;
const VFS_DT_LNK: u32 = 10;

#[inline]
fn align_up(x: usize, align: usize) -> usize {
    (x + align - 1) & !(align - 1)
}

pub static GLOBAL_INODE_WRITE_LOCKS: UPSafeCell<BTreeMap<u32, usize>> = unsafe {
    UPSafeCell::new(BTreeMap::new())
};

#[derive(Copy, Clone)]
pub struct OpenFlags {
    pub read: bool,
    pub write: bool,
    pub append: bool,
    pub create: bool,
    pub truncate: bool,
}

impl OpenFlags {
    pub const RDONLY: Self = Self {
        read: true,
        write: false,
        append: false,
        create: false,
        truncate: false,
    };

    pub const WRONLY: Self = Self {
        read: false,
        write: true,
        append: false,
        create: false,
        truncate: false,
    };

    pub const RDWR: Self = Self {
        read: true,
        write: true,
        append: false,
        create: false,
        truncate: false,
    };
}

pub struct OpenResult {
    pub fd: Arc<FileDescriptor>,
    /// 当请求 write 但写锁冲突时，会降级为只读打开；此时 `write_granted = false`。
    pub write_granted: bool,
}

///OS文件描述符
pub struct FileDescriptor{ //记得放入全局文件描述符表
    inner: UPSafeCell<Box<dyn FileDescriptorTrait>>,
    pub inode_num: u32,
    has_write_lock: bool,
    pub flags: OpenFlags,
}

struct OpenFileHandle {
    of: OpenFile,
}

impl OpenFileHandle {
    fn new(of: OpenFile) -> Self {
        Self { of }
    }
}

impl FileDescriptorTrait for OpenFileHandle {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, VfsFsError> {
        let mut rootfs_guard = ROOTFS.lock();
        let rootfs = rootfs_guard.as_mut().ok_or(VfsFsError::FsInnerError)?;
        let mut fs_guard = rootfs.fs.lock();
        let Ext4Fs { dev, fs } = &mut *fs_guard;
        let fs_inner = fs.as_mut().ok_or(VfsFsError::FsInnerError)?;

        let data = ext4_read_at(dev, fs_inner, &mut self.of, buf.len())
            .map_err(|_| VfsFsError::FsInnerError)?;
        let n = core::cmp::min(buf.len(), data.len());
        buf[..n].copy_from_slice(&data[..n]);
        Ok(n)
    }

    fn write(&mut self, buf: &[u8]) -> Result<usize, VfsFsError> {
        let mut rootfs_guard = ROOTFS.lock();
        let rootfs = rootfs_guard.as_mut().ok_or(VfsFsError::FsInnerError)?;
        let mut fs_guard = rootfs.fs.lock();
        let Ext4Fs { dev, fs } = &mut *fs_guard;
        let fs_inner = fs.as_mut().ok_or(VfsFsError::FsInnerError)?;

        ext4_write_at(dev, fs_inner, &mut self.of, buf).map_err(|_| VfsFsError::FsInnerError)?;
        Ok(buf.len())
    }

    fn read_at(&mut self, offset: usize, buf: &mut [u8]) -> Result<usize, VfsFsError> {
        let mut rootfs_guard = ROOTFS.lock();
        let rootfs = rootfs_guard.as_mut().ok_or(VfsFsError::FsInnerError)?;
        let mut fs_guard = rootfs.fs.lock();
        let Ext4Fs { dev, fs } = &mut *fs_guard;
        let fs_inner = fs.as_mut().ok_or(VfsFsError::FsInnerError)?;

        ext4_lseek(&mut self.of, offset as u64);
        let data = ext4_read_at(dev, fs_inner, &mut self.of, buf.len())
            .map_err(|_| VfsFsError::FsInnerError)?;
        let n = core::cmp::min(buf.len(), data.len());
        buf[..n].copy_from_slice(&data[..n]);
        Ok(n)
    }

    fn write_at(&mut self, offset: usize, buf: &[u8]) -> Result<usize, VfsFsError> {
        let mut rootfs_guard = ROOTFS.lock();
        let rootfs = rootfs_guard.as_mut().ok_or(VfsFsError::FsInnerError)?;
        let mut fs_guard = rootfs.fs.lock();
        let Ext4Fs { dev, fs } = &mut *fs_guard;
        let fs_inner = fs.as_mut().ok_or(VfsFsError::FsInnerError)?;

        ext4_lseek(&mut self.of, offset as u64);
        ext4_write_at(dev, fs_inner, &mut self.of, buf).map_err(|_| VfsFsError::FsInnerError)?;
        Ok(buf.len())
    }

    fn lseek(&mut self, offset: isize, whence: usize) -> Result<usize, VfsFsError> {
        let cur = self.of.offset as i64;
        let off = offset as i64;
        let new_off = match whence {
            0 => off,
            1 => cur.saturating_add(off),
            2 => {
                let end = self.of.inode.size() as i64;
                end.saturating_add(off)
            }
            _ => return Err(VfsFsError::FsInnerError),
        };
        if new_off < 0 {
            return Err(VfsFsError::FsInnerError);
        }
        ext4_lseek(&mut self.of, new_off as u64);
        Ok(self.of.offset as usize)
    }

    fn path(&self) -> Option<&str> {
        Some(&self.of.path)
    }

    fn offset(&self) -> Option<u64> {
        Some(self.of.offset)
    }

    fn set_offset(&mut self, off: u64) -> Result<(), VfsFsError> {
        self.of.offset = off;
        Ok(())
    }
}

fn inode_try_acquire_write_lock(inode_num: u32) -> bool {
    let mut table = GLOBAL_INODE_WRITE_LOCKS.lock();
    let cnt = table.get(&inode_num).copied().unwrap_or(0);
    if cnt != 0 {
        return false;
    }
    table.insert(inode_num, 1);
    true
}

fn inode_release_write_lock(inode_num: u32) {
    let mut table = GLOBAL_INODE_WRITE_LOCKS.lock();
    let Some(cnt) = table.get_mut(&inode_num) else {
        return;
    };
    if *cnt <= 1 {
        table.remove(&inode_num);
    } else {
        *cnt -= 1;
    }
}

impl Drop for FileDescriptor {
    fn drop(&mut self) {
        if self.has_write_lock {
            inode_release_write_lock(self.inode_num);
            self.has_write_lock = false;
        }
    }
}

impl FileDescriptor {

    ///特殊创建方式 不指定inodenum，无意义
    pub fn new(flags:OpenFlags,op:OpenFile)->Self{
        let mut has_write_lock:bool=false;//默认false
        if flags.append || flags.truncate || flags.write {
            has_write_lock = true;
        }
        Self {
            inner: UPSafeCell::new(Box::new(OpenFileHandle::new(op))),
            inode_num: 0,
            has_write_lock,
            flags,
        }
    }

    pub fn new_from_inner(flags: OpenFlags, has_write_lock: bool, inner: Box<dyn FileDescriptorTrait>) -> Self {
        Self {
            inner: UPSafeCell::new(inner),
            inode_num: 0,
            has_write_lock,
            flags,
        }
    }

    pub fn read_at(&self, offset: usize, buf: &mut [u8]) -> Result<usize, VfsFsError> {
        if !self.flags.read {
            return Err(VfsFsError::FsInnerError);
        }
        let mut inner = self.inner.lock();
        inner.read_at(offset, buf)
    }

    pub fn read(&self, buf: &mut [u8]) -> Result<usize, VfsFsError> {
        if !self.flags.read {
            return Err(VfsFsError::FsInnerError);
        }
        let mut inner = self.inner.lock();
        inner.read(buf)
    }

    pub fn write_at(&self, offset: usize, buf: &[u8]) -> Result<usize, VfsFsError> {
        if !self.flags.write || !self.has_write_lock {
            return Err(VfsFsError::FsInnerError);
        }
        let mut inner = self.inner.lock();
        inner.write_at(offset, buf)
    }

    pub fn write(&self, buf: &[u8]) -> Result<usize, VfsFsError> {
        if !self.flags.write || !self.has_write_lock {
            return Err(VfsFsError::FsInnerError);
        }
        let mut inner = self.inner.lock();
        inner.write(buf)
    }

    pub fn lseek(&self, offset: isize, whence: usize) -> Result<usize, VfsFsError> {
        let mut inner = self.inner.lock();
        inner.lseek(offset, whence)
    }

    fn dir_path_and_offset(&self) -> Result<(String, usize), VfsFsError> {
        let inner = self.inner.lock();
        let path = inner.path().ok_or(VfsFsError::FsInnerError)?;
        let off = inner.offset().ok_or(VfsFsError::FsInnerError)? as usize;
        Ok((path.to_string(), off))
    }

    fn set_stream_offset(&self, off: usize) -> Result<(), VfsFsError> {
        let mut inner = self.inner.lock();
        inner.set_offset(off as u64)
    }
}

/// 统一路径：绝对路径保持不变，相对路径以 ROOTFS.path 为前缀
fn normalize_path(path: &str) -> Result<String, VfsFsError> {
    let guard = ROOTFS.lock();
    let rootfs = guard.as_ref().ok_or(VfsFsError::FsInnerError)?;
    let cwd = &rootfs.path;

    if path.starts_with('/') {
        Ok(path.to_string())
    } else if cwd == "/" {
        Ok(format!("/{}", path))
    } else {
        Ok(format!("{}/{}", cwd, path))
    }
}

/// 计算父目录路径和子名（假定已是绝对路径，不为空且不为根）
fn split_parent_child(abs_path: &str) -> (String, String) {
    if abs_path == "/" {
        return ("/".to_string(), "".to_string());
    }
    match abs_path.rfind('/') {
        Some(0) => ("/".to_string(), abs_path[1..].to_string()),
        Some(pos) => {
            let parent = &abs_path[..pos];
            let child = &abs_path[pos + 1..];
            (parent.to_string(), child.to_string())
        }
        None => ("/".to_string(), abs_path.to_string()),
    }
}

/// open：根据绝对/相对路径返回一个被 Mutex/Arc 包裹的 VfsInode
pub fn vfs_open(path: &str, mut flags: OpenFlags) -> Result<OpenResult, VfsFsError> {
    let abs_path = match normalize_path(path) {
        Ok(p) => p,
        Err(e) => {
            error!("vfs_open: normalize_path failed: path={} err={}", path, e);
            return Err(e);
        }
    };

    let mut rootfs_guard = ROOTFS.lock();
    let rootfs = match rootfs_guard.as_mut() {
        Some(r) => r,
        None => {
            error!("vfs_open: ROOTFS not initialized: path={}", abs_path);
            return Err(VfsFsError::FsInnerError);
        }
    };
    let mut fs_guard = rootfs.fs.lock();
    let Ext4Fs { dev, fs } = &mut *fs_guard;
    let fs_inner = match fs.as_mut() {
        Some(f) => f,
        None => {
            error!("vfs_open: ext4 fs not mounted: path={}", abs_path);
            return Err(VfsFsError::FsInnerError);
        }
    };

    // 打开文件：create 语义由底层 open 的 create 参数承载
    let mut of = match ext4_open(dev, fs_inner, &abs_path, flags.create) {
        Ok(of) => of,
        Err(_) => {
            error!(
                "vfs_open: ext4_open failed: path={} create={} read={} write={} truncate={} append={}",
                abs_path, flags.create, flags.read, flags.write, flags.truncate, flags.append
            );
            return Err(VfsFsError::FsInnerError);
        }
    };

    // truncate 语义：如果请求 truncate 且允许写，则调用 truncate
    if flags.truncate {
        if flags.write {
            if ext4_truncate(dev, fs_inner, &abs_path, 0).is_err() {
                error!("vfs_open: ext4_truncate failed: path={}", abs_path);
                return Err(VfsFsError::FsInnerError);
            }
        } else {
            error!("vfs_open: truncate requested without write permission: path={}", abs_path);
            return Err(VfsFsError::FsInnerError);
        }
    }

    // append 语义：把 offset 调整到文件尾（依赖 rsext4 OpenFile 暴露 size 或通过 inode.size 获取）
    if flags.append {
        let end = of.inode.size() as u64;
        ext4_lseek(&mut of, end);
    }

    let inode_num = of.inode_num;
    let mut write_granted = flags.write;
    let mut has_write_lock = false;

    if flags.write {
        if inode_try_acquire_write_lock(inode_num) {
            has_write_lock = true;
        } else {
            // 写锁冲突：降级为只读
            flags.write = false;
            write_granted = false;
            debug!("vfs_open: write lock conflict, downgrade to read-only: path={} inode={}", abs_path, inode_num);
        }
    }

    //独立fd
    let fd = Arc::new(FileDescriptor {
        inner: UPSafeCell::new(Box::new(OpenFileHandle::new(of))),
        inode_num,
        has_write_lock,
        flags,
    });

    Ok(OpenResult { fd, write_granted })
}

/// read_at：从给定路径的文件指定 offset 读取
pub fn vfs_read_at(fd: &Arc<FileDescriptor>, offset: usize, buf: &mut [u8]) -> Result<usize, VfsFsError> {
    fd.read_at(offset, buf)
}

/// write_at：向给定路径的文件指定 offset 写入
pub fn vfs_write_at(fd: &Arc<FileDescriptor>, offset: usize, buf: &[u8]) -> Result<usize, VfsFsError> {
    fd.write_at(offset, buf)
}

/// mkdir：基于绝对或相对路径创建目录
pub fn vfs_mkdir(path: &str) -> Result<(), VfsFsError> {
    let abs = normalize_path(path)?;
    if abs == "/" {
        return Ok(());
    }
    let mut rootfs_guard = ROOTFS.lock();
    let rootfs = rootfs_guard.as_mut().ok_or(VfsFsError::FsInnerError)?;
    let mut fs_guard = rootfs.fs.lock();
    let Ext4Fs { dev, fs } = &mut *fs_guard;
    let fs_inner = fs.as_mut().ok_or(VfsFsError::FsInnerError)?;

    let res = ext4_mkdir(dev, fs_inner, &abs);
    if res.is_none() {
        return Err(VfsFsError::FsInnerError);
    }
    Ok(())
}

/// mkfile：基于绝对或相对路径创建文件
pub fn vfs_mkfile(path: &str) -> Result<(), VfsFsError> {
    let abs = normalize_path(path)?;
    if abs == "/" {
        return Err(VfsFsError::FsInnerError);
    }
    let mut rootfs_guard = ROOTFS.lock();
    let rootfs = rootfs_guard.as_mut().ok_or(VfsFsError::FsInnerError)?;
    let mut fs_guard = rootfs.fs.lock();
    let Ext4Fs { dev, fs } = &mut *fs_guard;
    let fs_inner = fs.as_mut().ok_or(VfsFsError::FsInnerError)?;

    let res = ext4_mkfile(dev, fs_inner, &abs, None, None);
    if res.is_none() {
        return Err(VfsFsError::FsInnerError);
    }
    Ok(())
}

/// mv：移动/重命名（高层按完整路径操作）
pub fn vfs_mv(src: &str, dest: &str) -> Result<(), VfsFsError> {
    let src_abs = normalize_path(src)?;
    let dest_abs = normalize_path(dest)?;
    let mut rootfs_guard = ROOTFS.lock();
    let rootfs = rootfs_guard.as_mut().ok_or(VfsFsError::FsInnerError)?;
    let mut fs_guard = rootfs.fs.lock();
    let Ext4Fs { dev, fs } = &mut *fs_guard;
    let fs_inner = fs.as_mut().ok_or(VfsFsError::FsInnerError)?;
    ext4_mv(fs_inner, dev, &src_abs, &dest_abs).map_err(|_| VfsFsError::FsInnerError)
}

/// rename：仅改变同一父目录下的名字（语义上等价于 mv 的子集）
pub fn vfs_rename(path: &str, new_name: &str) -> Result<(), VfsFsError> {
    let abs = normalize_path(path)?;
    if abs == "/" {
        return Err(VfsFsError::FsInnerError);
    }
    let new_path = if let Some(pos) = abs.rfind('/') {
        let parent = &abs[..pos];
        if parent.is_empty() {
            format!("/{new_name}")
        } else {
            format!("{parent}/{new_name}")
        }
    } else {
        new_name.to_string()
    };

    let mut rootfs_guard = ROOTFS.lock();
    let rootfs = rootfs_guard.as_mut().ok_or(VfsFsError::FsInnerError)?;
    let mut fs_guard = rootfs.fs.lock();
    let Ext4Fs { dev, fs } = &mut *fs_guard;
    let fs_inner = fs.as_mut().ok_or(VfsFsError::FsInnerError)?;
    ext4_rename(dev, fs_inner, &abs, &new_path).map_err(|_| VfsFsError::FsInnerError)
}

pub fn vfs_truncate(path: &str, size: u64) -> Result<(), VfsFsError> {
    let abs = normalize_path(path)?;
    if abs == "/" {
        return Err(VfsFsError::FsInnerError);
    }
    let mut rootfs_guard = ROOTFS.lock();
    let rootfs = rootfs_guard.as_mut().ok_or(VfsFsError::FsInnerError)?;
    let mut fs_guard = rootfs.fs.lock();
    let Ext4Fs { dev, fs } = &mut *fs_guard;
    let fs_inner = fs.as_mut().ok_or(VfsFsError::FsInnerError)?;
    ext4_truncate(dev, fs_inner, &abs, size).map_err(|_| VfsFsError::FsInnerError)
}

/// unlink：删除文件（不删除目录）
pub fn vfs_unlink(path: &str) -> Result<(), VfsFsError> {
    let abs = normalize_path(path)?;
    if abs == "/" {
        return Err(VfsFsError::FsInnerError);
    }

    let mut rootfs_guard = ROOTFS.lock();
    let rootfs = rootfs_guard.as_mut().ok_or(VfsFsError::FsInnerError)?;
    let mut fs_guard = rootfs.fs.lock();
    let Ext4Fs { dev, fs } = &mut *fs_guard;
    let fs_inner = fs.as_mut().ok_or(VfsFsError::FsInnerError)?;

    let got = get_inode_with_num(fs_inner, dev, &abs)
        .map_err(|_| VfsFsError::FsInnerError)?
        .ok_or(VfsFsError::FsInnerError)?;
    let (_ino, inode) = got;
    if inode.is_dir() {
        return Err(VfsFsError::FsInnerError);
    }

    ext4_unlink(fs_inner, dev, &abs);
    Ok(())
}

/// stat：获取路径的基本元数据
pub fn vfs_stat(path: &str) -> Result<VfsStat, VfsFsError> {
    let abs = normalize_path(path)?;

    let mut rootfs_guard = ROOTFS.lock();
    let rootfs = rootfs_guard.as_mut().ok_or(VfsFsError::FsInnerError)?;
    let mut fs_guard = rootfs.fs.lock();
    let Ext4Fs { dev, fs } = &mut *fs_guard;
    let fs_inner = fs.as_mut().ok_or(VfsFsError::FsInnerError)?;

    let got = get_inode_with_num(fs_inner, dev, &abs)
        .map_err(|_| VfsFsError::FsInnerError)?
        .ok_or(VfsFsError::FsInnerError)?;
    let (ino, inode) = got;

    let file_type = if inode.is_dir() {
        VFS_DT_DIR
    } else if inode.is_file() {
        VFS_DT_REG
    } else {
        VFS_DT_UNKNOWN
    };

    Ok(VfsStat {
        inode: ino,
        size: inode.size(),
        mode: 0,
        file_type,
    })
}

/// getdents64：读取目录项，按 linux_dirent64 格式编码；使用 OpenFile.offset 作为“字节偏移”保存位置。
///
/// linux_dirent64:
/// - d_ino(u64)
/// - d_off(u64)   (这里填“下一个 entry 的累计偏移”，不保证与内核一致，但能满足用户态顺序遍历)
/// - d_reclen(u16)
/// - d_type(u8)
/// - d_name(char[] + '\0')
pub fn vfs_getdents64(fd: &Arc<FileDescriptor>, max_len: usize) -> Result<Vec<u8>, VfsFsError> {
    if max_len == 0 {
        return Ok(Vec::new());
    }

    // 目录必须可读
    if !fd.flags.read {
        return Err(VfsFsError::FsInnerError);
    }

    let mut rootfs_guard = ROOTFS.lock();
    let rootfs = rootfs_guard.as_mut().ok_or(VfsFsError::FsInnerError)?;
    let mut fs_guard = rootfs.fs.lock();
    let Ext4Fs { dev, fs } = &mut *fs_guard;
    let fs_inner = fs.as_mut().ok_or(VfsFsError::FsInnerError)?;

    let (dir_path, stream_off) = fd.dir_path_and_offset()?;

    let got = get_inode_with_num(fs_inner, dev, &dir_path)
        .map_err(|_| VfsFsError::FsInnerError)?
        .ok_or(VfsFsError::FsInnerError)?;
    let (_dir_ino, mut dir_inode) = got;
    if !dir_inode.is_dir() {
        return Err(VfsFsError::FsInnerError);
    }

    // 把整个目录编码成一个连续的 dirent64 流，然后依据 OpenFile.offset 做分页返回。
    let blocks = resolve_inode_block_allextend(fs_inner, dev, &mut dir_inode)
        .map_err(|_| VfsFsError::FsInnerError)?;

    let mut stream: Vec<u8> = Vec::new();
    let mut cur_off: u64 = 0;
    let hdr_len = core::mem::size_of::<LinuxDirent64>();

    for &phys in blocks.values() {
        let cached = fs_inner
            .datablock_cache
            .get_or_load(dev, phys)
            .map_err(|_| VfsFsError::FsInnerError)?;
        let data = &cached.data[..BLOCK_SIZE];
        let iter = DirEntryIterator::new(data);
        for (entry, _) in iter {
            if entry.inode == 0 {
                continue;
            }

            // 跳过 "." 和 ".."，方便用户态直接 ls
            if entry.name == b"." || entry.name == b".." {
                continue;
            }

            let dtype = match entry.file_type {
                1 => VFS_DT_REG,
                2 => VFS_DT_DIR,
                7 => VFS_DT_LNK,
                _ => VFS_DT_UNKNOWN,
            };

            // 计算 record 长度：header + name + '\0'，再 8 字节对齐。
            let name_len = entry.name.len();
            let reclen = align_up(hdr_len + name_len + 1, 8);

            let base = stream.len();
            stream.resize(base + reclen, 0);
            cur_off = cur_off.saturating_add(reclen as u64);

            let hdr = LinuxDirent64 {
                d_ino: entry.inode as u64,
                d_off: cur_off,
                d_reclen: reclen as u16,
                d_type: dtype as u8,
            };
            let hdr_bytes: &[u8] = unsafe {
                core::slice::from_raw_parts(
                    (&hdr as *const LinuxDirent64) as *const u8,
                    core::mem::size_of::<LinuxDirent64>(),
                )
            };
            stream[base..base + hdr_bytes.len()].copy_from_slice(hdr_bytes);
            // d_name + '\0'
            let name_base = base + hdr_len;
            stream[name_base..name_base + name_len].copy_from_slice(entry.name);
            stream[name_base + name_len] = 0;
        }
    }

    if stream_off >= stream.len() {
        return Ok(Vec::new());
    }

    let end = core::cmp::min(stream.len(), stream_off + max_len);
    let out = stream[stream_off..end].to_vec();
    fd.set_stream_offset(end)?;
    Ok(out)
}

/// remove：删除给定路径的文件或目录
pub fn vfs_remove(path: &str) -> Result<(), VfsFsError> {
    let abs = normalize_path(path)?;

    // 不允许删除根目录
    if abs == "/" {
        return Err(VfsFsError::FsInnerError);
    }

    #[cfg(feature = "ext4")]
    {
        use crate::fs::fs_backend::Ext4Fs;
        use rsext4::ext4_backend::dir::get_inode_with_num;
        use rsext4::ext4_backend::file::{delete_dir, delete_file};

        let mut rootfs_guard = ROOTFS.lock();
        let rootfs = rootfs_guard.as_mut().ok_or(VfsFsError::FsInnerError)?;

        let mut fs_guard = rootfs.fs.lock();
        let Ext4Fs { dev, fs } = &mut *fs_guard;
        let fs_inner = fs.as_mut().ok_or(VfsFsError::FsInnerError)?;

        match get_inode_with_num(fs_inner, dev, &abs) {
            Ok(Some((_ino, inode))) => {
                if inode.is_dir() {
                    delete_dir(fs_inner, dev, &abs);
                } else {
                    delete_file(fs_inner, dev, &abs);
                }
                Ok(())
            }
            _ => Err(VfsFsError::FsInnerError),
        }
    }

    #[cfg(not(feature = "ext4"))]
    {
        let _ = path;
        Err(VfsFsError::FsInnerError)
    }
}
