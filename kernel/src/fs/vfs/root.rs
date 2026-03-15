#[cfg(feature = "ext4")]
use alloc::boxed::Box;
#[cfg(feature = "ext4")]
use alloc::collections::btree_map::BTreeMap;
use alloc::{string::String, sync::Arc};
use rsext4::mkfs;
use spin::Mutex;
use crate::config::{CONSENT, MB};
use crate::fs::vfs::{MountFs, OpenFlags, VfsFsError, vfs_open};
#[cfg(feature = "ext4")]
use crate::fs::fs_backend::{Ext4BlockDevice, Ext4Fs};
#[cfg(feature = "ext4")]
use crate::fs::fs_backend::RamFs;
use crate::fs::fs_backend::*;
#[cfg(feature = "ext4")]
use crate::fs::partition::{DevicePartition};
#[cfg(feature = "ext4")]
use crate::fs::partition::mbr::parsing_mbr_partition;
#[cfg(feature = "ext4")]
use crate::fs::partition::gpt::{parsing_gpt_header, parsing_gpt_entries};
#[cfg(feature = "ext4")]
use crate::fs::vfs::VBLOCK;
#[cfg(feature = "ext4")]
use crate::config::SECTOR_SIZE;
use crate::sync::UPSafeCell;
use crate::fs::vfs::vfs::VfsFs;
use lazy_static::lazy_static;
use log::{debug, error, warn};
use crate::alloc::string::ToString;
use crate::fs::vfs::{LinuxDirent64, VFS_DT_REG};
use crate::fs::fs_backend::fat32::Fat32Fs;
use crate::fs::vfs::{vfs_getdents64, vfs_mkdir, vfs_open as api_vfs_open, vfs_read_at, vfs_stat, vfs_write};
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

            // 根据平台选择块设备
            #[cfg(target_arch = "aarch64")]
            let blk: Arc<Mutex<dyn crate::fs::vfs::BlueBlk>> = {
                use crate::arch::driver::emmc_blk::kernel_impl::init_emmc_clk;
                use crate::arch::driver::emmc_blk::EmmcBlk;
                use crate::arch::driver::emmc_blk::EMMC_DWCMSHC_ADDR;
                let emmc_base = unsafe {
                    EMMC_DWCMSHC_ADDR
                };
                init_emmc_clk();
                if emmc_base==0 {
                    panic!("EMMC_DWCMSHC_ADDR is zero address,please register EMMC_DWCMSHC_ADDR on dtb probe!");
                }
                let emmc =EmmcBlk::new(emmc_base) //香橙派5 plus emmc地址
                    .expect("eMMC init failed");
                Arc::new(Mutex::new(emmc))
            };
            #[cfg(not(target_arch = "aarch64"))]
            #[cfg(feature = "ext4")]
            use crate::arch::driver::virtio_blk::VirtBlk;
            #[cfg(not(target_arch = "aarch64"))]
            let blk: Arc<Mutex<dyn crate::fs::vfs::BlueBlk>> = Arc::new(Mutex::new(VirtBlk::new()));
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
                .read_block(0, &mut mbr)
                .map_err(|_| VfsFsError::IO)?;

            let parts = parsing_mbr_partition(mbr);
            let mbr_ok = parts.as_ref().map_or(false, |v| !v.is_empty());
            debug!("MBR parse result: ok={}, count={}", mbr_ok,
                   parts.as_ref().map_or(0, |v| v.len()));

            if mbr_ok {
                // MBR 分区表有效
                let parts = parts.unwrap();
                for (idx, entry) in parts.into_iter().enumerate() {
                    debug!("MBR partition {}: start_lba={}, sectors={}", idx, entry.start_lbn, entry.len);
                    let dev = Arc::new(VBLOCK::new(blk.clone(), DevicePartition::MBR(entry)))
                        as Arc<dyn crate::fs::vfs::File>;
                    let path = alloc::format!("/vda{}", idx + 1);
                    ramfs.mkdev(path.as_str(), dev)?;
                }
            } else {
                // MBR 失败，尝试 GPT
                debug!("Trying GPT partition table...");
                let mut lba1: [u8; SECTOR_SIZE] = [0; SECTOR_SIZE];
                blk.lock()
                    .read_block(1, &mut lba1)
                    .map_err(|_| VfsFsError::IO)?;

                let (entry_lba, num_entries, entry_size) = match parsing_gpt_header(&lba1) {
                    Ok(v) => v,
                    Err(e) => {
                        error!("GPT header parse failed: {}", e);
                        return Err(VfsFsError::Invalid);
                    }
                };
                debug!("GPT header: entry_lba={}, num_entries={}, entry_size={}",
                       entry_lba, num_entries, entry_size);

                // 读取分区条目所占的扇区
                let entries_bytes = num_entries as usize * entry_size as usize;
                let entry_sectors = (entries_bytes + SECTOR_SIZE - 1) / SECTOR_SIZE;
                let mut entry_buf = alloc::vec![0u8; entry_sectors * SECTOR_SIZE];
                for s in 0..entry_sectors {
                    blk.lock()
                        .read_block(
                            entry_lba as usize + s,
                            &mut entry_buf[s * SECTOR_SIZE..(s + 1) * SECTOR_SIZE],
                        )
                        .map_err(|_| VfsFsError::IO)?;
                }

                let gpt_parts = parsing_gpt_entries(&entry_buf, num_entries, entry_size);
                debug!("GPT found {} partitions", gpt_parts.len());
                if gpt_parts.is_empty() {
                    return Err(VfsFsError::Invalid);
                }
                for (idx, gp) in gpt_parts.into_iter().enumerate() {
                    debug!("GPT partition {}: start_lba={}, sectors={}", idx, gp.start_lba, gp.sectors);
                    let dev = Arc::new(VBLOCK::new(blk.clone(), DevicePartition::GPT(gp)))
                        as Arc<dyn crate::fs::vfs::File>;
                    let path = alloc::format!("/vda{}", idx + 1);
                    ramfs.mkdev(path.as_str(), dev)?;
                }
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
        debug!("FileSysteam Initialing...");
        // 挂载ramfs
        let mut mount_point:BTreeMap<MountPath,MountFs> =BTreeMap::new(); 
        // WARN: 5MB RamFs
        let ramfs = RamFs::new(5*MB);
        let mount_fs:MountFs = Arc::new(Mutex::new(ramfs));
        // Mount to /
        mount_point.insert(MountPath("/".to_string()), mount_fs);

        let vfs_root = RootFs{
            mount_poinr:mount_point
        };

        // init fs
        *(ROOTFS.lock()) =Some(vfs_root); 

        // build vblock
        if Self::scan_and_build_vblock_device().is_err(){

            // verbose
            
            unsafe {
                CONSENT = true;
            }

            // 比赛fat32 sdcard环境
            warn!("Entern consent mode!");
            // 1) 将整盘 /vda (raw，无分区表) 挂载为 FAT32 到 /sd
            let vda = match vfs_open("/vda", OpenFlags::empty()) {
                Ok(f) => f,
                Err(e) => {
                    error!("consent mode: open /vda failed: {}", e);
                    return;
                }
            };

            let fat32 = match Fat32Fs::new(vda) {
                Ok(fs) => fs,
                Err(e) => {
                    error!("consent mode: Fat32Fs::new failed: {}", e);
                    return;
                }
            };
            let fat32_mnt: MountFs = Arc::new(Mutex::new(fat32));
            if fat32_mnt.lock().mount().is_err() {
                error!("consent mode: fat32 mount failed");
                return;
            }

            {
                let mut rootfs_guard = ROOTFS.lock();
                let root_mount_point = &mut rootfs_guard.as_mut().expect("root vfs not init").mount_poinr;
                root_mount_point.insert(MountPath("/sd".to_string()), fat32_mnt);
            }

            let _ = vfs_mkdir("/bin");
            let _ = vfs_mkdir("/mnt");
            let _ = vfs_mkdir("/dev");

            if let Ok(src) = vfs_open("/vda", OpenFlags::empty()) {
                let rootfs_guard = ROOTFS.lock();
                let root = rootfs_guard.as_ref().expect("root vfs not init");
                if let Ok(Some((fs, sub))) = root.resolve_mount_point("/") {
                    let _ = sub;
                    let mut fs_guard = fs.lock();
                    if let Some(ramfs) = fs_guard.as_any_mut().downcast_mut::<RamFs>() {
                        let _ = ramfs.mkdev("/dev/vda", src.clone());
                        let _ = ramfs.mkdev("/dev/vda2", src);
                    }
                }
            }

            // 比赛环境下保持根为 ramfs，并通过 /sd 直接访问官方提供的 sdcard 文件
            return;
        }

        #[cfg(feature = "ext4")]
        {
            use alloc::string::ToString;
            use crate::fs::vfs::vfs_mkdir;

            // 逐个探测分区，找到能挂载 ext4 的
            let mut mounted_fs: Option<Arc<Mutex<Ext4Fs>>> = None;
            for idx in 1..=8 {
                let path = alloc::format!("/vda{}", idx);
                let vda = match vfs_open(path.as_str(), OpenFlags::RDWR) {
                    Ok(f) => f,
                    Err(_) => break,
                };
                let ext4_dev = Ext4BlockDevice::new(vda);
                let fs = Arc::new(Mutex::new(Ext4Fs::new(ext4_dev)));
                if fs.lock().mount().is_ok() {
                    debug!("Found ext4 on {}", path);
                    mounted_fs = Some(fs);
                    break;
                }
                debug!("{} is not ext4, skipping", path);
            }

            let fs = mounted_fs.expect("No ext4 partition found on any vda*");

            let old_fs: Arc<Mutex<dyn VfsFs>>;
            {
                let mut rootfs_guard = ROOTFS.lock();
                let root_mount_point = &mut rootfs_guard.as_mut().expect("root vfs not init").mount_poinr;
                old_fs = root_mount_point.remove(&MountPath("/".to_string())).expect("Ramfs not mount at /");
                root_mount_point.insert(MountPath("/".to_string()), fs as Arc<Mutex<dyn VfsFs>>);
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