//!上层通用接口
use crate::alloc::string::ToString;
use crate::config::SECTOR_SIZE;
use crate::fs::partition::gpt::{parsing_gpt_entries, parsing_gpt_header};
use crate::fs::vfs::root::ROOTFS;
use crate::fs::vfs::{File, KStat, MountFs, OpenFlags, VfsFs, VfsFsError, VfsStat};
use crate::task::{TASK_MANAER, TASK_MANAGER_INIT};
use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use log::error;

fn resolve_mount(path: &str) -> Result<(MountFs, String, String), VfsFsError> {
    let abs = normalize_path(path)?;
    let (fs, sub) = ROOTFS.lock(|rootfs_guard| {
        let rootfs = rootfs_guard.as_mut().ok_or_else(|| {
            error!(
                "resolve_mount failed: ROOTFS not initialized (path={})",
                path
            );
            VfsFsError::IO
        })?;
        let (fs, sub) = rootfs
            .resolve_mount_point(&abs)
            .inspect_err(|&e| {
                error!(
                    "resolve_mount: resolve_mount_point failed: path={} err={:?}",
                    abs, e
                );
            })?
            .ok_or_else(|| {
                error!("resolve_mount: mount point not found: path={}", abs);
                VfsFsError::NotFound
            })?;
        Ok((fs, sub))
    })?;

    Ok((fs, abs, sub))
}

/// 解析 `/dev/vda1` 这类分区设备路径，返回 `(整盘路径, 1-based 分区号)`。
pub fn vfs_split_partition_device_path(abs_path: &str) -> Option<(&str, usize)> {
    if !abs_path.starts_with("/dev/") {
        return None;
    }
    let mut end = abs_path.len();
    while end > 0 && abs_path.as_bytes()[end - 1].is_ascii_digit() {
        end -= 1;
    }
    if end == abs_path.len() {
        return None;
    }
    let idx_str = &abs_path[end..];
    let idx = idx_str.parse::<usize>().ok()?;
    if idx == 0 {
        return None;
    }
    Some((&abs_path[..end], idx))
}

/// 分区文件系统类型提示。
///
/// 这里表达的是“从分区表里推断出的文件系统倾向”，
/// 供上层做 `auto` 挂载决策使用。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartitionFsHint {
    Ext4,
    Fat32,
    Fat16,
    Unknown,
}

/// 读取分区类型提示，统一兼容 MBR 和 GPT。
pub fn vfs_read_partition_type(
    disk: &Arc<dyn File>,
    part_idx_1based: usize,
) -> Result<PartitionFsHint, VfsFsError> {
    const MBR_OFFSET: usize = 0x1BE;
    const MBR_ENTRY_SIZE: usize = 16;
    const GPT_LINUX_FILESYSTEM_GUID: [u8; 16] = [
        0xaf, 0x3d, 0xc6, 0x0f, 0x83, 0x84, 0x72, 0x47, 0x8e, 0x79, 0x3d, 0x69, 0xd8, 0x47, 0x7d,
        0xe4,
    ];
    const GPT_MICROSOFT_BASIC_DATA_GUID: [u8; 16] = [
        0xa2, 0xa0, 0xd0, 0xeb, 0xe5, 0xb9, 0x33, 0x44, 0x87, 0xc0, 0x68, 0xb6, 0xb7, 0x26, 0x99,
        0xc7,
    ];
    const GPT_EFI_SYSTEM_GUID: [u8; 16] = [
        0x28, 0x73, 0x2a, 0xc1, 0x1f, 0xf8, 0xd2, 0x11, 0xba, 0x4b, 0x00, 0xa0, 0xc9, 0x3e, 0xc9,
        0x3b,
    ];

    fn map_gpt_guid_to_hint(guid: &[u8; 16]) -> PartitionFsHint {
        if guid == &GPT_LINUX_FILESYSTEM_GUID {
            PartitionFsHint::Ext4
        } else if guid == &GPT_MICROSOFT_BASIC_DATA_GUID || guid == &GPT_EFI_SYSTEM_GUID {
            PartitionFsHint::Fat32
        } else {
            PartitionFsHint::Unknown
        }
    }

    fn map_mbr_partition_type_to_hint(ptype: u8) -> PartitionFsHint {
        match ptype {
            0x83 => PartitionFsHint::Ext4,
            0x0b | 0x0c => PartitionFsHint::Fat32,
            0x0e => PartitionFsHint::Fat16,
            _ => PartitionFsHint::Unknown,
        }
    }

    if part_idx_1based == 0 {
        return Err(VfsFsError::Invalid);
    }

    let mut mbr = [0u8; SECTOR_SIZE];
    disk.read_at(0, &mut mbr)?;
    if mbr[510] != 0x55 || mbr[511] != 0xAA {
        return Err(VfsFsError::Invalid);
    }

    if part_idx_1based <= 4 {
        let base = MBR_OFFSET + (part_idx_1based - 1) * MBR_ENTRY_SIZE;
        let ptype = mbr[base + 4];
        if ptype != 0x00 && ptype != 0xee {
            return Ok(map_mbr_partition_type_to_hint(ptype));
        }
    }

    let protective_gpt = (0..4usize).any(|idx| {
        let base = MBR_OFFSET + idx * MBR_ENTRY_SIZE;
        mbr[base + 4] == 0xee
    });
    if !protective_gpt {
        return Err(VfsFsError::Invalid);
    }

    let mut lba1 = [0u8; SECTOR_SIZE];
    disk.read_at(SECTOR_SIZE, &mut lba1)?;
    let (entry_lba, num_entries, entry_size) =
        parsing_gpt_header(&lba1).map_err(|_| VfsFsError::Invalid)?;
    if part_idx_1based > num_entries as usize {
        return Err(VfsFsError::Invalid);
    }

    let entries_bytes = num_entries as usize * entry_size as usize;
    let entry_sectors = entries_bytes.div_ceil(SECTOR_SIZE);
    let mut entry_buf = alloc::vec![0u8; entry_sectors * SECTOR_SIZE];
    for sector_idx in 0..entry_sectors {
        disk.read_at(
            (entry_lba as usize + sector_idx) * SECTOR_SIZE,
            &mut entry_buf[sector_idx * SECTOR_SIZE..(sector_idx + 1) * SECTOR_SIZE],
        )?;
    }

    let parts = parsing_gpt_entries(&entry_buf, num_entries, entry_size);
    let entry = parts.get(part_idx_1based - 1).ok_or(VfsFsError::Invalid)?;
    Ok(map_gpt_guid_to_hint(&(entry.metadata.type_guid.0)))
}

/// 统一路径：绝对路径保持不变，相对路径以 进程打开的路径 为前缀
/// TASK_MANAER初始化期间只能用绝对路径，内核也不应该出现相对路径
pub fn normalize_path(path: &str) -> Result<String, VfsFsError> {
    let combin = if path.starts_with('/') {
        path.to_string()
    } else {
        let tsmn_init: bool;
        unsafe {
            tsmn_init = TASK_MANAGER_INIT;
        }
        if tsmn_init {
            let cwd = TASK_MANAER.get_current_cwd();
            format!("{}/{}", cwd, path)
        } else {
            format!("/{}", path)
        }
    };

    let mut parts: Vec<&str> = Vec::new();
    for pa in combin.split('/') {
        if pa.is_empty() || pa == "." {
            continue;
        }
        if pa == ".." {
            parts.pop();
            continue;
        }
        parts.push(pa);
    }
    if parts.is_empty() {
        Ok("/".to_string())
    } else {
        Ok(format!("/{}", parts.join("/")))
    }
}

pub fn vfs_open(path: &str, flags: OpenFlags) -> Result<Arc<dyn File>, VfsFsError> {
    let (mnt, abs_path, sub_path) = resolve_mount(path)?;
    let mut guard = mnt.lock();
    let file = guard
        .open(mnt.clone(), &sub_path, flags)
        .inspect_err(|&e| {
            error!("vfs_open failed: path={} err={:?}", abs_path, e);
        })?;
    Ok(file)
}

pub fn vfs_read(file: &Arc<dyn File>, buf: &mut [u8]) -> Result<usize, VfsFsError> {
    file.read(buf)
}

pub fn vfs_write(file: &Arc<dyn File>, buf: &[u8]) -> Result<usize, VfsFsError> {
    file.write(buf)
}

pub fn vfs_read_at(
    file: &Arc<dyn File>,
    offset: usize,
    buf: &mut [u8],
) -> Result<usize, VfsFsError> {
    file.read_at(offset, buf)
}

pub fn vfs_write_at(file: &Arc<dyn File>, offset: usize, buf: &[u8]) -> Result<usize, VfsFsError> {
    file.write_at(offset, buf)
}

pub fn vfs_lseek(file: &Arc<dyn File>, offset: isize, whence: usize) -> Result<usize, VfsFsError> {
    file.lseek(offset, whence)
}

pub fn vfs_getdents64(file: &Arc<dyn File>, max_len: usize) -> Result<Vec<u8>, VfsFsError> {
    file.getdents64(max_len)
}

pub fn vfs_fstat(file: &Arc<dyn File>) -> Result<VfsStat, VfsFsError> {
    file.stat()
}

pub fn vfs_fstat_kstat(file: &Arc<dyn File>) -> Result<KStat, VfsFsError> {
    Ok(file.stat()?.into())
}

/// mkdir：基于绝对或相对路径创建目录
pub fn vfs_mkdir(path: &str) -> Result<(), VfsFsError> {
    let (mnt, abs, sub) = resolve_mount(path)?;
    if abs == "/" {
        return Ok(());
    }
    let mut guard = mnt.lock();
    guard.mkdir(&sub).inspect_err(|&e| {
        error!("vfs_mkdir failed: path={} err={:?}", abs, e);
    })
}

/// mkfile：基于绝对或相对路径创建文件
pub fn vfs_mkfile(path: &str) -> Result<(), VfsFsError> {
    let (mnt, abs, sub) = resolve_mount(path)?;
    if abs == "/" {
        return Err(VfsFsError::Invalid);
    }
    let mut guard = mnt.lock();
    guard.mkfile(&sub).inspect_err(|&e| {
        error!("vfs_mkfile failed: path={} err={:?}", abs, e);
    })
}

/// mv：移动/重命名（高层按完整路径操作）
pub fn vfs_mv(src: &str, dest: &str) -> Result<(), VfsFsError> {
    let (src_mnt, _src_abs, src_sub) = resolve_mount(src)?;
    let (dst_mnt, _dst_abs, dst_sub) = resolve_mount(dest)?;
    if !Arc::ptr_eq(&src_mnt, &dst_mnt) {
        return Err(VfsFsError::NotSupported);
    }
    let mut guard = src_mnt.lock();
    guard.mv(&src_sub, &dst_sub).inspect_err(|&e| {
        error!("vfs_mv failed: src={} err={:?}", src, e);
    })
}

/// rename：仅改变同一父目录下的名字（语义上等价于 mv 的子集）
pub fn vfs_rename(path: &str, new_name: &str) -> Result<(), VfsFsError> {
    let (mnt, abs, sub) = resolve_mount(path)?;
    if abs == "/" {
        return Err(VfsFsError::Invalid);
    }
    let new_path = if let Some(pos) = abs.rfind('/') {
        let parent = &abs[..pos];
        if parent.is_empty() {
            format!("/{new_name}")
        } else {
            format!("{parent}/{new_name}")
        }
    } else {
        new_name.to_string()
    };

    let _ = new_path;
    let mut guard = mnt.lock();
    guard.rename(&sub, new_name).inspect_err(|&e| {
        error!(
            "vfs_rename failed: path={} new_name={} err={:?}",
            path, new_name, e
        );
    })
}

pub fn vfs_truncate(path: &str, size: u64) -> Result<(), VfsFsError> {
    let (mnt, abs, sub) = resolve_mount(path)?;
    if abs == "/" {
        return Err(VfsFsError::Invalid);
    }
    let mut guard = mnt.lock();
    guard.truncate(&sub, size).inspect_err(|&e| {
        error!(
            "vfs_truncate failed: path={} size={} err={:?}",
            path, size, e
        );
    })
}

/// unlink：删除文件（不删除目录）
pub fn vfs_unlink(path: &str) -> Result<(), VfsFsError> {
    let (mnt, abs, sub) = resolve_mount(path)?;
    if abs == "/" {
        return Err(VfsFsError::Invalid);
    }
    let mut guard = mnt.lock();
    guard.unlink(&sub).inspect_err(|&e| {
        error!("vfs_unlink failed: path={} err={:?}", path, e);
    })
}

/// stat：获取路径的基本元数据
pub fn vfs_stat(path: &str) -> Result<VfsStat, VfsFsError> {
    let (mnt, abs, sub) = resolve_mount(path)?;
    let mut guard = mnt.lock();
    guard.stat(&sub).inspect_err(|&e| {
        error!("vfs_stat failed: path={} err={:?}", abs, e);
    })
}

/// remove：删除给定路径的文件
pub fn vfs_remove(path: &str) -> Result<(), VfsFsError> {
    let (mnt, abs, sub) = resolve_mount(path)?;

    // 不允许删除根目录
    if abs == "/" {
        return Err(VfsFsError::Invalid);
    }

    let mut guard = mnt.lock();
    match guard.stat(&sub) {
        Ok(st) => {
            if st.file_type == crate::fs::vfs::VFS_DT_DIR {
                error!("vfs_remove failed: path={} is a directory", abs);
                Err(VfsFsError::NotSupported)
            } else {
                guard.unlink(&sub).inspect_err(|&e| {
                    error!("vfs_remove unlink failed: path={} err={:?}", abs, e);
                })
            }
        }
        Err(e) => {
            error!("vfs_remove stat failed: path={} err={:?}", abs, e);
            Err(e)
        }
    }
}
