use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::any::Any;
use bitflags::bitflags;
use spin::Mutex;
use crate::fs::vfs::vfserror::{VfsFsError};

pub type MountFs = Arc<Mutex<dyn VfsFs>>;

pub enum EntryType {
    File,
    Dir
}

bitflags! {
    #[derive(Debug,Clone, Copy)]

    pub struct OpenFlags: usize {
        const RONLY = 0;
        const WRONLY = 1 << 0;
        const RDWR = 1 << 1;
        const CREAT = 1 << 6;
        const TRUNC = 1 << 9;
        const APPEND = 1 << 10;
        const DIRECTORY = 1 << 21;
    }
}

impl OpenFlags {
    pub const ACCMODE_MASK: usize = 0x3;

    pub fn accmode(self) -> usize {
        self.bits() & Self::ACCMODE_MASK
    }

    pub fn readable(self) -> bool {
        self.accmode() != 0x001
    }

    pub fn writable(self) -> bool {
        matches!(self.accmode(), 0x001 | 0x002)
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

pub const VFS_DT_UNKNOWN: u32 = 0;
pub const VFS_DT_REG: u32 = 8;
pub const VFS_DT_DIR: u32 = 4;
pub const VFS_DT_LNK: u32 = 10;

#[repr(C, packed)]
#[derive(Clone, Copy, Debug, Default)]
pub struct LinuxDirent64 {
    pub d_ino: u64,
    pub d_off: u64,
    pub d_reclen: u16,
    pub d_type: u8,
    // ..filename
}

pub trait File: Send + Sync {
    fn read(&self, buf: &mut [u8]) -> Result<usize, VfsFsError>;
    fn write(&self, buf: &[u8]) -> Result<usize, VfsFsError>;

    fn read_at(&self, _offset: usize, _buf: &mut [u8]) -> Result<usize, VfsFsError> {
        Err(VfsFsError::NotSupported)
    }

    fn write_at(&self, _offset: usize, _buf: &[u8]) -> Result<usize, VfsFsError> {
        Err(VfsFsError::NotSupported)
    }

    fn lseek(&self, _offset: isize, _whence: usize) -> Result<usize, VfsFsError> {
        Err(VfsFsError::NotSupported)
    }

    fn getdents64(&self, _max_len: usize) -> Result<Vec<u8>, VfsFsError> {
        Err(VfsFsError::NotSupported)
    }

    fn stat(&self) -> Result<VfsStat, VfsFsError> {
        Err(VfsFsError::NotSupported)
    }

    fn flush(&self) -> Result<(), VfsFsError> {
        Ok(())
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct KStat {
    pub st_dev: u64,
    pub st_ino: u64,
    pub st_mode: u32,
    pub st_nlink: u32,
    pub st_uid: u32,
    pub st_gid: u32,
    pub st_rdev: u64,
    pub __pad: u64,
    pub st_size: i64,
    pub st_blksize: u32,
    pub __pad2: i32,
    pub st_blocks: u64,
    pub st_atime_sec: i64,
    pub st_atime_nsec: i64,
    pub st_mtime_sec: i64,
    pub st_mtime_nsec: i64,
    pub st_ctime_sec: i64,
    pub st_ctime_nsec: i64,
    pub __unused: [u32; 2],
}

impl From<VfsStat> for KStat {
    fn from(v: VfsStat) -> Self {
        let size_i64 = core::cmp::min(v.size, i64::MAX as u64) as i64;
        let blocks = (v.size + 511) / 512;
        Self {
            st_dev: 0,
            st_ino: v.inode as u64,
            st_mode: v.mode,
            st_nlink: 1,
            st_uid: 0,
            st_gid: 0,
            st_rdev: 0,
            __pad: 0,
            st_size: size_i64,
            st_blksize: 4096,
            __pad2: 0,
            st_blocks: blocks,
            st_atime_sec: 0,
            st_atime_nsec: 0,
            st_mtime_sec: 0,
            st_mtime_nsec: 0,
            st_ctime_sec: 0,
            st_ctime_nsec: 0,
            __unused: [0, 0],
        }
    }
}

/// 
pub trait VfsFs :Send + Sync{
    fn mount(&mut self)->Result<(),VfsFsError>;
    fn umount(&mut self)->Result<(),VfsFsError>;
    fn name(&self)->Result<String,VfsFsError>;

    fn mkdir(&mut self, _path: &str) -> Result<(), VfsFsError> {
        Err(VfsFsError::NotSupported)
    }

    fn mkfile(&mut self, _path: &str) -> Result<(), VfsFsError> {
        Err(VfsFsError::NotSupported)
    }

    fn mv(&mut self, _src: &str, _dest: &str) -> Result<(), VfsFsError> {
        Err(VfsFsError::NotSupported)
    }

    fn rename(&mut self, _path: &str, _new_name: &str) -> Result<(), VfsFsError> {
        Err(VfsFsError::NotSupported)
    }

    fn open(
        &mut self,
        _mount_fs: MountFs,
        _path: &str,
        _flags: OpenFlags,
    ) -> Result<Arc<dyn File>, VfsFsError> {
        Err(VfsFsError::NotSupported)
    }

    fn truncate(&mut self, _path: &str, _size: u64) -> Result<(), VfsFsError> {
        Err(VfsFsError::NotSupported)
    }

    fn unlink(&mut self, _path: &str) -> Result<(), VfsFsError> {
        Err(VfsFsError::NotSupported)
    }

    fn stat(&mut self, _path: &str) -> Result<VfsStat, VfsFsError> {
        Err(VfsFsError::NotSupported)
    }

    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}