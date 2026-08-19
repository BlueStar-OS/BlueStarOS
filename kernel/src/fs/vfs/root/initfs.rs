use super::{MountPath, RootFs, ROOTFS};
use crate::config::MB;
use crate::fs::component::devices::{framebuffer_device_file, keyboard_device_file};
use crate::fs::fs_backend::{Ext4BlockDevice, Ext4Fs, RamFs};
use crate::fs::vfs::vfs::VfsFs;
use crate::fs::vfs::{vfs_mkdir, MountFs};
use alloc::collections::btree_map::BTreeMap;
use alloc::string::ToString;
use alloc::sync::Arc;
use log::{debug, error};
use spin::Mutex;

impl RootFs {
    /// 在启动期 ramfs 根上注册内建设备节点。
    ///
    /// 设备先创建在 ramfs 根目录；切换到 ext4 根后，这个 ramfs 会整体挂到
    /// `/dev`，因此 `/fb0` 和 `/keyboard` 会自然变成 `/dev/fb0` 与
    /// `/dev/keyboard`。
    fn register_bootstrap_device_nodes() {
        let Some((fs, _)) = ROOTFS.lock(|rootfs_guard| {
            let root = rootfs_guard.as_ref().expect("root vfs not init");
            root.resolve_mount_point("/").ok().flatten()
        }) else {
            error!("register_bootstrap_device_nodes: root mount point missing");
            return;
        };

        let mut fs_guard = fs.lock();
        let Some(ramfs) = fs_guard.as_any_mut().downcast_mut::<RamFs>() else {
            error!("register_bootstrap_device_nodes: root fs is not ramfs");
            return;
        };

        if let Err(err) = ramfs.mkdev("/fb0", framebuffer_device_file()) {
            error!(
                "register_bootstrap_device_nodes: create /fb0 failed: {}",
                err
            );
        }
        if let Err(err) = ramfs.mkdev("/keyboard", keyboard_device_file()) {
            error!(
                "register_bootstrap_device_nodes: create /keyboard failed: {}",
                err
            );
        }
    }

    /// 初始化根文件系统，并切换到磁盘上的 ext4 根分区。
    ///
    /// 启动流程：
    /// 1. 创建一个最小 ramfs 作为临时根文件系统；
    /// 2. 注册启动早期需要的设备节点；
    /// 3. 扫描块设备并创建对应的 VFS 设备节点；
    /// 4. 寻找 GPT 名称为 `rootfs` 的 ext4 分区；
    /// 5. 将 ext4 切换为 `/`，并把原 ramfs 挂到 `/dev`。
    pub fn init_rootfs() {
        debug!("FileSystem initializing...");

        let mut mount_point: BTreeMap<MountPath, MountFs> = BTreeMap::new();
        let ramfs = RamFs::new(5 * MB);
        let mount_fs: MountFs = Arc::new(Mutex::new(ramfs));
        mount_point.insert(MountPath("/".to_string()), mount_fs);
        ROOTFS.lock(|root| {
            *root = Some(RootFs {
                mount_poinr: mount_point,
            });
        });

        Self::register_bootstrap_device_nodes();

        if Self::scan_and_build_vblock_device().is_err() {
            panic!("failed to scan block devices while initializing rootfs");
        }

        Self::mount_ext4_root();
    }

    /// 找到可挂载的 ext4 根分区，并把启动 ramfs 移到 `/dev`。
    fn mount_ext4_root() {
        let mut mounted_ext4_root: Option<Arc<Mutex<Ext4Fs>>> = None;
        let mut probe_candidates = Self::collect_filesystem_probe_candidates();
        probe_candidates.sort_by_key(|candidate| !candidate.is_gpt_rootfs);

        if !probe_candidates
            .iter()
            .any(|candidate| candidate.is_gpt_rootfs)
        {
            panic!("cannot find GPT partition named rootfs");
        }

        for candidate in probe_candidates {
            if !candidate.is_gpt_rootfs {
                continue;
            }

            let ext4_dev = Ext4BlockDevice::new(candidate.device);
            let fs = Arc::new(Mutex::new(Ext4Fs::new(ext4_dev)));
            if fs.lock().mount().is_ok() {
                debug!("Found ext4 rootfs on {}", candidate.path);
                mounted_ext4_root = Some(fs);
                break;
            }
            debug!("{} is not a mountable ext4 rootfs, skipping", candidate.path);
        }

        let fs = mounted_ext4_root.expect("no mountable ext4 rootfs found");

        let old_fs: Arc<Mutex<dyn VfsFs>> = ROOTFS.lock(|rootfs_guard| {
            let root_mount_point = &mut rootfs_guard
                .as_mut()
                .expect("root vfs not init")
                .mount_poinr;
            let old = root_mount_point
                .remove(&MountPath("/".to_string()))
                .expect("ramfs not mounted at /");
            root_mount_point.insert(MountPath("/".to_string()), fs as Arc<Mutex<dyn VfsFs>>);
            old
        });

        vfs_mkdir("/dev").expect("/dev create failed");
        ROOTFS.lock(|rootfs_guard| {
            let root_mount_point = &mut rootfs_guard
                .as_mut()
                .expect("root vfs not init")
                .mount_poinr;
            root_mount_point.insert(MountPath("/dev/".to_string()), old_fs);
        });
    }
}
