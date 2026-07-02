//! sys_mount — 挂载块设备分区到已有目录。
//!
//! ## 作用
//! 挂载块设备分区到已有目录。
//!
//! ## 参数
//! `source_ptr` 源路径；`target_ptr` 挂载点；`fstype_ptr` 文件系统名；`flags/data` 挂载选项。
//!
//! ## 注意事项
//! 仅支持当前 VFS 具备的 ext4/fat32 路径；多数 mount flags/data 未实现。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: fs/namespace.c:4206
//!
//! ## 实现情况
//! 已实现 BlueStarOS VFS 路径。

use crate::fs::fs_backend::fat32::Fat32Fs;
use crate::fs::fs_backend::{Ext4BlockDevice, Ext4Fs};
use crate::fs::vfs::{normalize_path, vfs_open, vfs_stat, OpenFlags, VFS_DT_DIR};
use crate::root::MountPath;
use crate::root::ROOTFS;
use crate::syscall::syscall::*;
use crate::VfsFs;
use alloc::string::String;
use alloc::sync::Arc;
use spin::Mutex;

pub fn sys_mount(
    source_ptr: usize,
    target_ptr: usize,
    fstype_ptr: usize,
    _flags: usize,
    _data_ptr: usize,
) -> isize {
    if target_ptr == 0 || fstype_ptr == 0 {
        error!(
            "sys_mount: invalid args target_ptr={:#x} fstype_ptr={:#x}",
            target_ptr, fstype_ptr
        );
        return BlueErr::EINVAL.as_isize();
    }

    let source = if source_ptr == 0 {
        String::new()
    } else {
        match read_c_string_from_user(source_ptr) {
            Ok(s) => s,
            Err(e) => {
                error!("sys_mount: invalid source ptr={:#x} err={}", source_ptr, e);
                return BlueErr::EFAULT.as_isize();
            }
        }
    };

    let target = match read_c_string_from_user(target_ptr) {
        Ok(s) => s,
        Err(e) => {
            error!("sys_mount: invalid target ptr={:#x} err={}", target_ptr, e);
            return BlueErr::EFAULT.as_isize();
        }
    };

    let fstype = match read_c_string_from_user(fstype_ptr) {
        Ok(s) => s,
        Err(e) => {
            error!("sys_mount: invalid fstype ptr={:#x} err={}", fstype_ptr, e);
            return BlueErr::EFAULT.as_isize();
        }
    };

    debug!(
        "sys_mount: source='{}' target='{}' fstype='{}'",
        source, target, fstype
    );

    // 规范化 target 路径，并要求其必须是目录
    let abs_target = match normalize_path(&target) {
        Ok(p) => p,
        Err(e) => {
            error!(
                "sys_mount: normalize target failed target={} err={:?}",
                target, e
            );
            return BlueErr::ENOENT.as_isize();
        }
    };
    if abs_target == "/" {
        // 不允许覆盖根挂载点
        error!("sys_mount: refuse to mount on /");
        return BlueErr::EINVAL.as_isize();
    }
    let st = match vfs_stat(&abs_target) {
        Ok(s) => s,
        Err(e) => {
            error!(
                "sys_mount: target stat failed target={} err={}",
                abs_target, e
            );
            return BlueErr::ENOENT.as_isize();
        }
    };
    if st.file_type != VFS_DT_DIR {
        error!(
            "sys_mount: target is not dir target={} type={}",
            abs_target, st.file_type
        );
        return BlueErr::ENOTDIR.as_isize();
    }

    let abs_source = match normalize_path(&source) {
        Ok(p) => p,
        Err(e) => {
            error!(
                "sys_mount: normalize source failed source={} err={:?}",
                source, e
            );
            return BlueErr::ENOENT.as_isize();
        }
    };
    if abs_source.is_empty() {
        error!("sys_mount: empty source");
        return BlueErr::ENOENT.as_isize();
    }

    let (disk_path, part_idx) = match crate::fs::vfs::vfs_split_partition_device_path(&abs_source) {
        Some(v) => v,
        None => {
            error!(
                "sys_mount: unsupported source path (expect /dev/xxxN) abs_source={}",
                abs_source
            );
            return BlueErr::ENODEV.as_isize();
        }
    };

    debug!(
        "sys_mount: parsed source abs_source={} disk_path={} part_idx={}",
        abs_source, disk_path, part_idx
    );

    let disk = match vfs_open(disk_path, OpenFlags::empty()) {
        Ok(f) => f,
        Err(e) => {
            error!(
                "sys_mount: open disk failed disk_path={} err={}",
                disk_path, e
            );
            return BlueErr::ENOENT.as_isize();
        }
    };
    let partition_fs_hint = match crate::fs::vfs::vfs_read_partition_type(&disk, part_idx) {
        Ok(t) => t,
        Err(e) => {
            error!(
                "sys_mount: read partition type failed disk_path={} part_idx={} err={}",
                disk_path, part_idx, e
            );
            return BlueErr::EIO.as_isize();
        }
    };

    debug!("sys_mount: partition fs hint={:?}", partition_fs_hint);

    let auto_fs = match partition_fs_hint {
        crate::fs::vfs::PartitionFsHint::Ext4 => "ext4",
        crate::fs::vfs::PartitionFsHint::Fat32 => "fat32",
        crate::fs::vfs::PartitionFsHint::Fat16 => "fat16",
        crate::fs::vfs::PartitionFsHint::Unknown => "unknown",
    };

    let is_auto = fstype.is_empty() || fstype == "auto";
    let explicit_fs = match fstype.as_str() {
        "vfat" => "fat32",
        other => other,
    };
    let req_fs = if is_auto { auto_fs } else { explicit_fs };
    debug!(
        "sys_mount: auto_fs={} req_fs={} hint={:?}",
        auto_fs, req_fs, partition_fs_hint
    );

    // POSIX 语义：若用户显式指定了 fstype，则按用户指定尝试挂载。
    // 只有 fstype=auto 时才依赖分区类型做自动判定。
    if is_auto {
        if req_fs == "fat16" || req_fs == "unknown" {
            error!(
                "sys_mount: unsupported fs req_fs={} hint={:?}",
                req_fs, partition_fs_hint
            );
            return BlueErr::ENODEV.as_isize();
        }
    } else {
        if req_fs != "ext4" && req_fs != "fat32" {
            error!(
                "sys_mount: unsupported explicit fstype={} hint={:?}",
                explicit_fs, partition_fs_hint
            );
            return BlueErr::ENODEV.as_isize();
        }
    }

    let src_dev = match vfs_open(&abs_source, OpenFlags::empty()) {
        Ok(f) => f,
        Err(e) => {
            error!(
                "sys_mount: open source device failed abs_source={} err={}",
                abs_source, e
            );
            return BlueErr::ENODEV.as_isize();
        }
    };
    let new_fs: Arc<Mutex<dyn VfsFs>> = match req_fs {
        "ext4" => {
            #[cfg(feature = "ext4")]
            {
                let blk = Ext4BlockDevice::new(src_dev);
                Arc::new(Mutex::new(Ext4Fs::new(blk))) as Arc<Mutex<dyn VfsFs>>
            }
            #[cfg(not(feature = "ext4"))]
            {
                error!("sys_mount: ext4 requested but ext4 feature is disabled");
                return BlueErr::ENODEV.as_isize();
            }
        }
        "fat32" => {
            let fs = match Fat32Fs::new(src_dev) {
                Ok(v) => v,
                Err(e) => {
                    error!("sys_mount: fat32 init failed err={}", e);
                    return BlueErr::EIO.as_isize();
                }
            };
            Arc::new(Mutex::new(fs)) as Arc<Mutex<dyn VfsFs>>
        }
        _ => return BlueErr::ENODEV.as_isize(),
    };
    // TODO(IRQ-unsafe): spin::Mutex<dyn VfsFs> held during mount() does not disable IRQ. VFS operations from IRQ context would deadlock.
    if let Err(e) = new_fs.lock().mount() {
        error!("sys_mount: fs.mount failed req_fs={} err={}", req_fs, e);
        return BlueErr::EIO.as_isize();
    }

    ROOTFS.lock(|root| {
        let rootfs = match root.as_mut() {
            Some(r) => r,
            None => {
                error!("sys_mount: ROOTFS not initialized");
                return BlueErr::ENODEV.as_isize();
            }
        };
        let key = MountPath(abs_target);
        if rootfs.mount_poinr.contains_key(&key) {
            error!("sys_mount: target already mounted target={}", key.0);
            return BlueErr::EBUSY.as_isize();
        }
        rootfs.mount_poinr.insert(key, new_fs);
        0
    })
}
