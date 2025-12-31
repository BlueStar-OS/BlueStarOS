#[cfg(feature = "ext4")]
pub mod ext4_backend;
use alloc::boxed::Box;
use alloc::sync::Arc;
#[cfg(feature = "ext4")]
pub use ext4_backend::*;
use spin::Mutex;

use crate::{driver::VirtBlk, fs::vfs::VfsFs};


pub fn get_main_fs() -> Arc<Mutex<Box<dyn VfsFs>>>{
//use cfg feature consider init whitch fs.
    let raw_block_dev = VirtBlk::new();
    let ext4_wrapping_blockdev = Ext4BlockDevice::new(raw_block_dev);
    Arc::new(Mutex::new(Box::new(Ext4Fs::new(ext4_wrapping_blockdev))))
}