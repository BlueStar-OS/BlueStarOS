#[cfg(feature = "ext4")]
use alloc::boxed::Box;
#[cfg(feature = "ext4")]
use alloc::collections::btree_map::BTreeMap;
use alloc::{string::String, sync::Arc};
use rsext4::mkfs;
use spin::Mutex;
use crate::config::MB;
use crate::fs::vfs::{MountFs, OpenFlags, VfsFsError, vfs_open};
#[cfg(feature = "ext4")]
use crate::driver::VirtBlk;
#[cfg(feature = "ext4")]
use crate::fs::fs_backend::{Ext4BlockDevice, Ext4Fs};
#[cfg(feature = "ext4")]
use crate::fs::fs_backend::RamFs;
#[cfg(feature = "ext4")]
use crate::fs::partition::{DevicePartition};
#[cfg(feature = "ext4")]
use crate::fs::partition::mbr::parsing_mbr_partition;
#[cfg(feature = "ext4")]
use crate::fs::vfs::VBLOCK;
#[cfg(feature = "ext4")]
use crate::config::SECTOR_SIZE;
use crate::sync::UPSafeCell;
use crate::fs::vfs::vfs::VfsFs;
use lazy_static::lazy_static;
use log::error;
use crate::alloc::string::ToString;
/// 全局根文件系统
lazy_static!{
pub static ref ROOTFS: UPSafeCell<Option<RootFs>> = UPSafeCell::new(None);
}

/// 挂载点路径
#[derive(Clone,Debug,PartialEq, Eq, PartialOrd)]
pub struct MountPath(pub String);

impl Ord for MountPath {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        //解析/个数
        let self_deep = self.0.chars().filter(|c|{*c == '/'}).count();
        let other_deep = other.0.chars().filter(|c|{*c == '/'}).count();
        if self_deep > other_deep {
            return core::cmp::Ordering::Less;//深度逆向排序
        }else if self_deep < other_deep {
            return core::cmp::Ordering::Greater;//深度逆向排序
        }else {
            return self.0.cmp(&other.0);
        }
    }
}


//全局虚拟文件系统
#[cfg(feature = "ext4")]
pub struct RootFs{
    pub mount_poinr:BTreeMap<MountPath,Arc<Mutex<dyn VfsFs>>>,// 挂载点
}

#[cfg(not(feature = "ext4"))]
pub struct RootFs{
    path:String, //当前路径
}

//虚拟根文件系统
impl RootFs {

    fn normalize_abs_path(path: &str) -> String {
        // Assume `path` is already absolute or caller guarantees it.
        // Collapse repeated '/', and remove trailing '/' (except root).
        let mut out = String::new();
        let mut prev_slash = false;
        for ch in path.chars() {
            if ch == '/' {
                if !prev_slash {
                    out.push('/');
                }
                prev_slash = true;
            } else {
                out.push(ch);
                prev_slash = false;
            }
        }
        if out.is_empty() {
            out.push('/');
        }
        while out.len() > 1 && out.ends_with('/') {
            out.pop();
        }
        out
    }

    fn is_component_prefix(mount: &str, path: &str) -> bool {
        let mount = if mount.len() > 1 {
            mount.trim_end_matches('/')
        } else {
            mount
        };

        if mount == "/" {
            return path.starts_with('/');
        }
        if path == mount {
            return true;
        }
        if path.starts_with(mount) {
            return path.as_bytes().get(mount.len()) == Some(&b'/');
        }
        false
    }

    /// 解析挂载点和剩余路径
    pub fn resolve_mount_point(
        &self,
        path: &str,
    ) -> Result<Option<(Arc<Mutex<dyn VfsFs>> , String)>, VfsFsError> {
        let abs = Self::normalize_abs_path(path);

        let mut best: Option<(usize, Arc<Mutex<dyn VfsFs>>, String)> = None;
        for (mp, fs) in self.mount_poinr.iter() {
            let mps = Self::normalize_abs_path(mp.0.as_str());
            if !Self::is_component_prefix(&mps, abs.as_str()) {
                continue;
            }

            let sub = if mps == "/" {
                abs.clone()
            } else if abs.len() == mps.len() {
                "/".to_string()
            } else {
                // abs starts with "{mps}/..."
                abs[mps.len()..].to_string()
            };

            let score = mps.len();
            match &best {
                Some((best_score, _, _)) if *best_score >= score => {}
                _ => best = Some((score, fs.clone(), sub)),
            }
        }

        Ok(best.map(|(_, fs, sub)| (fs, sub)))
    }

    pub fn scan_and_build_vblock_device()->Result<(),VfsFsError>{
        #[cfg(feature = "ext4")]
        {
            let root = ROOTFS.lock();
            let root = root.as_ref().ok_or(VfsFsError::IO)?;
            let (fs, sub) = root
                .resolve_mount_point("/")?
                .ok_or(VfsFsError::NotFound)?;
            if sub != "/" {
                return Err(VfsFsError::IO);
            }

            let mut guard = fs.lock();
            let ramfs = guard
                .as_any_mut()
                .downcast_mut::<RamFs>()
                .ok_or(VfsFsError::NotSupported)?;

            let blk = Arc::new(Mutex::new(VirtBlk::new()));
            let total_sectors = blk.lock().capacity_in_sectors();

            let whole = Arc::new(VBLOCK::new(
                blk.clone(),
                DevicePartition::Raw {
                    base_lba: 0,
                    sectors: total_sectors,
                },
            )) as Arc<dyn crate::fs::vfs::File>;
            ramfs.mkdev("/vda", whole)?;

            let mut mbr: [u8; SECTOR_SIZE] = [0; SECTOR_SIZE];
            blk.lock()
                .0
                .lock()
                .read_block(0, &mut mbr)
                .map_err(|_| VfsFsError::IO)?;

            let parts = parsing_mbr_partition(mbr).map_err(|_| VfsFsError::Invalid)?;
            for (idx, entry) in parts.into_iter().enumerate() {
                let dev = Arc::new(VBLOCK::new(blk.clone(), DevicePartition::MBR(entry)))
                    as Arc<dyn crate::fs::vfs::File>;
                let path = alloc::format!("/vda{}", idx + 1);
                ramfs.mkdev(path.as_str(), dev)?;
            }
            Ok(())
        }

        #[cfg(not(feature = "ext4"))]
        {
            Err(VfsFsError::NotSupported)
        }
    }

    //initfs 根据feature选择fs实例化
    pub fn init_rootfs(){
        // 挂载ramfs
        let mut mount_point:BTreeMap<MountPath,MountFs> =BTreeMap::new(); 
        // WARN: 1MB RamFs
        let ramfs = RamFs::new(1*MB);
        let mount_fs:MountFs = Arc::new(Mutex::new(ramfs));
        // Mount to /
        mount_point.insert(MountPath("/".to_string()), mount_fs);

        let vfs_root = RootFs{
            mount_poinr:mount_point
        };

        // init fs
        *(ROOTFS.lock()) =Some(vfs_root); 

        // build vblock
        Self::scan_and_build_vblock_device().expect("Vblock build failed!");

        // select first vblock use this fs and init mainfs,mount to /,umount ramfs and remount to /dev
        let vda1 = vfs_open("/vda1", OpenFlags::RDWR).expect("Can't find any vblock device");

        #[cfg(feature = "ext4")]
        {
            //初始化全局虚拟文件系统

            use alloc::string::ToString;

            use crate::fs::vfs::vfs_mkdir;
            let ext4_wrapping_blockdev = Ext4BlockDevice::new(vda1);
            let old_fs:Arc<Mutex<dyn VfsFs>>;
        {
            let fs = Arc::new(Mutex::new(Ext4Fs::new(ext4_wrapping_blockdev)));
            fs.lock().mount().expect("ext4 mount failed");
            let mut rootfs_guard = ROOTFS.lock();
            let root_mount_point = &mut rootfs_guard.as_mut().expect("root vfs not init").mount_poinr;
            old_fs = root_mount_point.remove(&MountPath("/".to_string())).expect("Ramfs not mount at /");
            root_mount_point.insert(MountPath("/".to_string()), fs.clone() as Arc<Mutex<dyn VfsFs>>);
        }   
            //make dev dir
            vfs_mkdir("/dev").expect("/dev create failed!");
            let mut rootfs_guard = ROOTFS.lock();
            let root_mount_point = &mut rootfs_guard.as_mut().expect("root vfs not init").mount_poinr;
            root_mount_point.insert(MountPath("/dev/".to_string()), old_fs);
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