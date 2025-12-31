use alloc::{string::String, sync::Arc};
use rsext4::mkfs;
use spin::Mutex;
use crate::fs::vfs::VfsFsError;
#[cfg(feature = "ext4")]
use crate::driver::VirtBlk;
#[cfg(feature = "ext4")]
use crate::fs::fs_backend::{Ext4BlockDevice, Ext4Fs};
use crate::sync::UPSafeCell;
use crate::fs::vfs::vfs::VfsFs;
use lazy_static::lazy_static;
use log::error;
///全局根文件系统
lazy_static!{
pub static ref ROOTFS: UPSafeCell<Option<RootFs>> = UPSafeCell::new(None);
}


//全局虚拟文件系统
#[cfg(feature = "ext4")]
pub struct RootFs{
    pub fs:Arc<Mutex<Ext4Fs>>, //根文件系统
    pub path:String, //当前路径
}

#[cfg(not(feature = "ext4"))]
pub struct RootFs{
    path:String, //当前路径
}

//虚拟根文件系统
impl RootFs {
    pub fn vfs_mkfs(&mut self){
        mkfs(&mut self.fs.lock().dev);
    }
    //initfs 根据feature选择fs实例化
    pub fn init_rootfs(){
        #[cfg(feature = "ext4")]
        {
            //初始化全局虚拟文件系统
            let raw_block_dev = VirtBlk::new();
            let ext4_wrapping_blockdev = Ext4BlockDevice::new(raw_block_dev);

            let fs = Arc::new(Mutex::new(Ext4Fs::new(ext4_wrapping_blockdev)));
            fs.lock().mount().expect("ext4 mount failed");

            let rootfs = RootFs {
                fs,
                path: String::from("/"),
            };
            *(ROOTFS.lock()) = Some(rootfs);
        }

        #[cfg(not(feature = "ext4"))]
        {
            error!("ext4 not turn");
            let rootfs = RootFs {
                path: String::from("/"),
            };
            *ROOTFS.lock() = Some(rootfs);
        }
    }

}