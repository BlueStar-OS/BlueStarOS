use alloc::{boxed::Box, vec::Vec};
use alloc::string::String;

use crate::fs::vfs::vfserror::{VfsFsError, VfsInodeError};

pub enum EntryType {
    File,
    Dir
}

pub trait VfsInode:Send +Sync {}

pub trait VfsFs {
    fn mount(&mut self)->Result<(),VfsFsError>;
    fn umount(&mut self)->Result<(),VfsFsError>;
    fn name(&self)->Result<String,VfsFsError>;
}