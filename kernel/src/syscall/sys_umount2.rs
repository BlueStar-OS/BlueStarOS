//! sys_umount2 — 卸载指定挂载点。
//!
//! ## 作用
//! 卸载指定挂载点。
//!
//! ## 参数
//! `target_ptr` 挂载点路径；`flags` 卸载标志。
//!
//! ## 注意事项
//! flags 当前忽略；忙检测只覆盖 cwd 和子挂载点。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: fs/namespace.c:2036-2058
//!
//! ## 实现情况
//! 已实现基础路径。

use crate::fs::vfs::normalize_path;
use crate::root::MountPath;
use crate::root::ROOTFS;
use crate::syscall::syscall::*;

pub fn sys_umount2(target_ptr: usize, _flags: usize) -> isize {
    if target_ptr == 0 {
        return BlueErr::EINVAL.as_isize();
    }
    let target = match read_c_string_from_user(target_ptr) {
        Ok(s) => s,
        Err(e) => {
            error!(
                "sys_umount2: invalid target ptr={:#x} err={}",
                target_ptr, e
            );
            return BlueErr::EFAULT.as_isize();
        }
    };
    let abs_target = match normalize_path(&target) {
        Ok(p) => p,
        Err(_) => return BlueErr::ENOENT.as_isize(),
    };
    if abs_target == "/" {
        return BlueErr::EINVAL.as_isize();
    }

    let key = MountPath(abs_target);

    ROOTFS.lock(|root| {
        let rootfs = match root.as_mut() {
            Some(r) => r,
            None => return BlueErr::ENODEV.as_isize(),
        };

        // 遍历进程列表确保任何进程不在挂载点路径上
        let mp_busy = TASK_MANAER.task_que_inner.lock(|inner| {
            inner
                .task_queen
                .iter()
                .any(|task| task.lock(|t| t.cwd.starts_with(&key.0)))
        }) || rootfs
            .mount_poinr
            .iter()
            .find(|mt| (*mt.0) != key && (*mt.0 .0).starts_with(&key.0))
            .is_some();

        if mp_busy {
            error!("[sys_umount]: Vblock:{} busy!", &key.0);
            return BlueErr::EBUSY.as_isize();
        }

        let Some(fs) = rootfs.mount_poinr.remove(&key) else {
            return BlueErr::ENOENT.as_isize();
        };

        if let Err(e) = fs.lock().umount() {
            error!("sys_umount2: fs.umount failed err={}", e);
            return BlueErr::EIO.as_isize();
        }
        0
    })
}
