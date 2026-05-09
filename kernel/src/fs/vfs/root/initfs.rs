use super::{MountPath, RootFs, ROOTFS};
use crate::config::{CONSENT, MB};
use crate::fs::component::devices::{framebuffer_device_file, keyboard_device_file};
use crate::fs::fs_backend::fat32::Fat32Fs;
use crate::fs::fs_backend::{Ext4BlockDevice, Ext4Fs, RamFs};
use crate::fs::vfs::vfs::VfsFs;
use crate::fs::vfs::{vfs_mkdir, vfs_open, MountFs, OpenFlags};
use alloc::collections::btree_map::BTreeMap;
use alloc::string::ToString;
use alloc::sync::Arc;
use log::{debug, error, warn};
use spin::Mutex;

impl RootFs {
    /// 在启动期 ramfs 根上注册内建设备节点。
    ///
    /// 这里故意创建为 `/fb0`、`/keyboard`，而不是 `/dev/fb0`：
    /// 后续 ext4 根切换成功后，整个旧 ramfs 会被重新挂到 `/dev`，
    /// 因而它的根目录项会自然变成最终系统视角里的 `/dev/fb0`
    /// 和 `/dev/keyboard`。
    fn register_bootstrap_device_nodes() {
        let rootfs_guard = ROOTFS.lock();
        let root = rootfs_guard.as_ref().expect("root vfs not init");
        let Ok(Some((fs, _))) = root.resolve_mount_point("/") else {
            error!("register_bootstrap_device_nodes: root mount point missing");
            return;
        };
        drop(rootfs_guard);

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

    /// 在“保持 ramfs 为最终根”的降级模式里，把根目录设备镜像到 `/dev`。
    ///
    /// 因为这条路径不会发生“旧 ramfs 挂回 `/dev`”的动作，所以要手工再创建一份
    /// `/dev/fb0` 与 `/dev/keyboard`。
    fn mirror_bootstrap_devices_into_dev_dir() {
        for (source_path, target_path) in [("/fb0", "/dev/fb0"), ("/keyboard", "/dev/keyboard")] {
            let Ok(source_file) = vfs_open(source_path, OpenFlags::empty()) else {
                error!(
                    "mirror_bootstrap_devices_into_dev_dir: open {} failed",
                    source_path
                );
                continue;
            };

            let rootfs_guard = ROOTFS.lock();
            let root = rootfs_guard.as_ref().expect("root vfs not init");
            let Ok(Some((fs, _))) = root.resolve_mount_point("/") else {
                error!("mirror_bootstrap_devices_into_dev_dir: root mount point missing");
                continue;
            };
            drop(rootfs_guard);

            let mut fs_guard = fs.lock();
            let Some(ramfs) = fs_guard.as_any_mut().downcast_mut::<RamFs>() else {
                error!("mirror_bootstrap_devices_into_dev_dir: root fs is not ramfs");
                continue;
            };

            if let Err(err) = ramfs.mkdev(target_path, source_file) {
                error!(
                    "mirror_bootstrap_devices_into_dev_dir: create {} failed: {}",
                    target_path, err
                );
            }
        }
    }

    /// 初始化根文件系统，并在可能时切换到 ext4 根分区。
    ///
    /// 入口流程总览：
    /// 1. 先创建一个 ramfs，并把它挂到 `/`，作为启动期的临时根文件系统；
    /// 2. 扫描已经注册的块设备，在这个临时根里创建 `/vda`、`/vda1` 这类块设备节点；
    /// 3. 如果扫描失败，则进入降级模式，保持 ramfs 为 `/`，并尝试把 FAT32 整盘挂到 `/sd`；
    /// 4. 如果扫描成功，则继续寻找可以挂载的 ext4 标识为 `rootfs` 的 GPT 根分区；
    /// 5. 找到 ext4 根分区后，用 ext4 替换 `/`，同时把旧 ramfs 挪到 `/dev`。
    pub fn init_rootfs() {
        debug!("FileSysteam Initialing...");

        // Step 1:
        // 先准备一个最小可用的 ramfs 挂到 `/`。
        // 这样即使后面的磁盘根分区暂时还没准备好，VFS 也已经有了一个可工作的根目录。
        let mut mount_point: BTreeMap<MountPath, MountFs> = BTreeMap::new();

        let ramfs = RamFs::new(5 * MB);
        let mount_fs: MountFs = Arc::new(Mutex::new(ramfs));
        mount_point.insert(MountPath("/".to_string()), mount_fs);
        let vfs_root = RootFs {
            mount_poinr: mount_point,
        };
        *(ROOTFS.lock()) = Some(vfs_root);

        // Step 1.1:
        // 把 framebuffer/keyboard 这类“启动早期就已经存在”的内建设备节点挂到
        // bootstrap ramfs 根目录。后续若成功切换 ext4 根，整个 ramfs 会作为 `/dev`
        // 回挂，不需要重复创建。
        Self::register_bootstrap_device_nodes();

        // Step 2:
        // 遍历 GLOBAL_BLOCKS 中已经注册的块设备，
        // 在当前 ramfs 根下创建原始盘节点和分区节点。
        if Self::scan_and_build_vblock_device().is_err() {
            // Step 3:
            // 如果块设备扫描失败，则无法切换到正式磁盘根，
            // 此时进入兼容/降级模式，继续使用 ramfs 作为 `/`。
            Self::enter_consent_mode();
            return;
        }

        // Step 4:
        // 块设备节点准备完成后，继续从候选设备中找一个
        // 可挂载且 GPT 标识为 `rootfs` 的 ext4 根分区。
        Self::mount_ext4_root();
    }

    /// 比赛环境兼容模式：保持 ramfs 为根，并把整盘 FAT32 挂到 `/sd`。
    fn enter_consent_mode() {
        // Step 3.1:
        // 设置降级模式标志，后续系统逻辑可据此知道当前不是标准 ext4 根环境。
        unsafe {
            CONSENT = true;
        }

        warn!("Entern consent mode!");

        // Step 3.2:
        // 尝试直接打开整盘 `/vda`，把它作为 FAT32 设备使用。
        let vda = match vfs_open("/vda", OpenFlags::empty()) {
            Ok(f) => f,
            Err(e) => {
                error!("consent mode: open /vda failed: {}", e);
                return;
            }
        };

        // Step 3.3:
        // 将整盘包装成 FAT32 文件系统，并准备挂载到 `/sd`。
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

        // Step 3.4:
        // 把 `/sd` 挂载点加入根挂载表。
        // 这里只做挂载表更新，写完立刻释放 ROOTFS 锁，避免后续 VFS 操作递归拿锁。
        {
            let mut rootfs_guard = ROOTFS.lock();
            let root_mount_point = &mut rootfs_guard
                .as_mut()
                .expect("root vfs not init")
                .mount_poinr;
            root_mount_point.insert(MountPath("/sd".to_string()), fat32_mnt);
        }

        // Step 3.5:
        // 在 ramfs 根下补齐常用目录，保证降级模式下基础路径存在。
        let _ = vfs_mkdir("/bin");
        let _ = vfs_mkdir("/mnt");
        let _ = vfs_mkdir("/dev");
        Self::mirror_bootstrap_devices_into_dev_dir();

        // Step 3.6:
        // 把 `/vda` 镜像到 `/dev` 下，便于调试以及用户态按设备路径访问。
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
    }

    /// 从文件系统探测候选中找到可挂载的 ext4 分区，并把原始 ramfs 挪到 `/dev`。
    fn mount_ext4_root() {
        // Step 4.1:
        // 遍历候选块设备：
        // - 先尝试整盘 Raw
        // - 再尝试 MBR/GPT 分区
        // 尝试 GPT 分区名为 `rootfs` 的候选；
        // 找不到就拒绝挂载
        // 找到就挂载为根分区
        let mut mounted_ext4_root: Option<Arc<Mutex<Ext4Fs>>> = None;
        let mut probe_candidates = Self::collect_filesystem_probe_candidates();
        probe_candidates.sort_by_key(|candidate| !candidate.is_gpt_rootfs);

        if probe_candidates
            .iter()
            .find(|candidate| candidate.is_gpt_rootfs)
            .is_none()
        {
            error!("Can not find rootfs gpt partitiion,refuse mount");
            panic!();
        }

        for candidate in probe_candidates {
            let ext4_dev = Ext4BlockDevice::new(candidate.device);
            let fs = Arc::new(Mutex::new(Ext4Fs::new(ext4_dev)));
            if fs.lock().mount().is_ok() {
                debug!("Found ext4 on {}", candidate.path);
                mounted_ext4_root = Some(fs);
                break;
            }
            debug!("{} is not ext4, skipping", candidate.path);
        }

        let fs =
            mounted_ext4_root.expect("No ext4 filesystem found on any registered block device");

        // Step 4.2:
        // 用找到的 ext4 替换当前 `/`。
        // 这里不能直接丢掉旧 ramfs，因为里面还承载着前期创建好的设备节点。
        let old_fs: Arc<Mutex<dyn VfsFs>>;
        {
            let mut rootfs_guard = ROOTFS.lock();
            let root_mount_point = &mut rootfs_guard
                .as_mut()
                .expect("root vfs not init")
                .mount_poinr;
            old_fs = root_mount_point
                .remove(&MountPath("/".to_string()))
                .expect("Ramfs not mount at /");
            root_mount_point.insert(MountPath("/".to_string()), fs as Arc<Mutex<dyn VfsFs>>);
        }

        // Step 4.3:
        // 在新的 ext4 根上创建 `/dev`，然后把旧 ramfs 回挂到 `/dev`。
        // 这样设备节点仍然由 ramfs 承载，但整个系统根目录已经切换为 ext4。
        vfs_mkdir("/dev").expect("/dev create failed!");
        let mut rootfs_guard = ROOTFS.lock();
        let root_mount_point = &mut rootfs_guard
            .as_mut()
            .expect("root vfs not init")
            .mount_poinr;
        root_mount_point.insert(MountPath("/dev/".to_string()), old_fs);
    }
}
