//!上层通用接口
use alloc::string::String;
use alloc::sync::Arc;
use crate::alloc::string::ToString;
use log::error;
use spin::Mutex;
use crate::task::TASK_MANAER;
use crate::fs::vfs::{File, KStat, MountFs, OpenFlags, ROOTFS, VfsFs, VfsFsError, VfsStat};
use alloc::format;
use alloc::vec::Vec;

fn resolve_mount(path: &str) -> Result<(MountFs, String, String), VfsFsError> {
    let abs = normalize_path(path)?;
    let mut rootfs_guard = ROOTFS.lock();
    let rootfs = rootfs_guard.as_mut().ok_or(VfsFsError::IO)?;
    let (fs, sub) = rootfs
        .resolve_mount_point(&abs)?
        .ok_or(VfsFsError::NotFound)?;
    Ok((fs, abs, sub))
}

/// 统一路径：绝对路径保持不变，相对路径以 进程打开的路径 为前缀
/// TASK_MANAER初始化期间只能用绝对路径，内核也不应该出现相对路径
pub fn normalize_path(path: &str) -> Result<String, VfsFsError> {
    let combin = if path.starts_with('/') {
        path.to_string()
    } else {
        let cwd = TASK_MANAER.get_current_cwd();
        format!("{}/{}", cwd, path)
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
    let file = guard.open(mnt.clone(), &sub_path, flags).map_err(|e| {
        error!("vfs_open failed: path={} err={:?}", abs_path, e);
        e
    })?;
    Ok(file)
}

pub fn vfs_read(file: &Arc<dyn File>, buf: &mut [u8]) -> Result<usize, VfsFsError> {
    file.read(buf)
}

pub fn vfs_write(file: &Arc<dyn File>, buf: &[u8]) -> Result<usize, VfsFsError> {
    file.write(buf)
}

pub fn vfs_read_at(file: &Arc<dyn File>, offset: usize, buf: &mut [u8]) -> Result<usize, VfsFsError> {
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
    guard.mkdir(&sub)
}

/// mkfile：基于绝对或相对路径创建文件
pub fn vfs_mkfile(path: &str) -> Result<(), VfsFsError> {
    let (mnt, abs, sub) = resolve_mount(path)?;
    if abs == "/" {
        return Err(VfsFsError::Invalid);
    }
    let mut guard = mnt.lock();
    guard.mkfile(&sub)
}

/// mv：移动/重命名（高层按完整路径操作）
pub fn vfs_mv(src: &str, dest: &str) -> Result<(), VfsFsError> {
    let (src_mnt, _src_abs, src_sub) = resolve_mount(src)?;
    let (dst_mnt, _dst_abs, dst_sub) = resolve_mount(dest)?;
    if !Arc::ptr_eq(&src_mnt, &dst_mnt) {
        return Err(VfsFsError::NotSupported);
    }
    let mut guard = src_mnt.lock();
    guard.mv(&src_sub, &dst_sub)
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
    guard.rename(&sub, new_name)
}

pub fn vfs_truncate(path: &str, size: u64) -> Result<(), VfsFsError> {
    let (mnt, abs, sub) = resolve_mount(path)?;
    if abs == "/" {
        return Err(VfsFsError::Invalid);
    }
    let mut guard = mnt.lock();
    guard.truncate(&sub, size)
}

/// unlink：删除文件（不删除目录）
pub fn vfs_unlink(path: &str) -> Result<(), VfsFsError> {
    let (mnt, abs, sub) = resolve_mount(path)?;
    if abs == "/" {
        return Err(VfsFsError::Invalid);
    }
    let mut guard = mnt.lock();
    guard.unlink(&sub)
}

/// stat：获取路径的基本元数据
pub fn vfs_stat(path: &str) -> Result<VfsStat, VfsFsError> {
    let (mnt, _abs, sub) = resolve_mount(path)?;
    let mut guard = mnt.lock();
    guard.stat(&sub)
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
                Err(VfsFsError::NotSupported)
            } else {
                guard.unlink(&sub)
            }
        }
        Err(e) => Err(e),
    }
}
