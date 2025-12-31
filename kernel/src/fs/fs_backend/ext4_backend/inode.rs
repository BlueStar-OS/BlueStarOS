use alloc::{boxed::Box, string::{String, ToString}, sync::Arc};
use alloc::format;
use log::error;
use rsext4::{
    OpenFile,read as ext4_read, lseek as ext4_lseek, mkdir, mkfile, mv as ext4_mv, open as ext4_open, read_at as ext4_read_at, rename as ext4_rename, write_at as ext4_write_at
};
use spin::Mutex;
use crate::{fs::{fs_backend::Ext4Fs, vfs::{VfsInode, VfsInodeError}}, sync::UPSafeCell};
use alloc::vec::Vec;
pub struct VfsExt4Inode{
    inode:UPSafeCell<OpenFile>,
}

impl VfsExt4Inode {
    pub fn new(inode:UPSafeCell<OpenFile>)->Self{
        VfsExt4Inode { inode }
    }
}

//TODO:基于rsext4的api给VfsExt4Inode实现vfsinode trait. 标记一下就行，底层会提供所有操作
impl VfsInode for VfsExt4Inode {}

