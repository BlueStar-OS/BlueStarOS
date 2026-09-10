//! Minimal system-information filesystem mounts.
//!
//! This is deliberately smaller than Linux sysfs.  It owns only stable,
//! read-only text views needed by early user-space tools.

pub mod dev;

use crate::config::MB;
use crate::fs::fs_backend::RamFs;
use crate::fs::vfs::root::{MountPath, ROOTFS};
use crate::fs::vfs::{vfs_mkdir, MountFs, VfsFs, VfsFsError};
use alloc::string::ToString;
use alloc::sync::Arc;
use log::error;
use spin::Mutex;

/// Mount the small read-only system-information tree at `/sys`.
///
/// Layout for now:
/// ```text
/// /sys/bus/usb/devices/      USB device directory
/// /sys/bus/usb/devices/list  one text record per enumerated device
/// ```
/// The RAM filesystem only supplies the directory hierarchy.  `list` is a
/// dynamic device file and reads the registry in `dev` directly.
pub fn init() {
    match vfs_mkdir("/sys") {
        Ok(()) | Err(VfsFsError::AlreadyExists) => {}
        Err(error) => {
            error!("sysfs: cannot create /sys: {}", error);
            return;
        }
    }

    let sysfs: MountFs = Arc::new(Mutex::new(RamFs::new(MB)));
    let setup = {
        let mut guard = sysfs.lock();
        let Some(ramfs) = guard.as_any_mut().downcast_mut::<RamFs>() else {
            error!("sysfs: internal RAM filesystem type mismatch");
            return;
        };
        ramfs
            .mount()
            .and_then(|()| ramfs.mkdir("/bus"))
            .and_then(|()| ramfs.mkdir("/bus/usb"))
            .and_then(|()| ramfs.mkdir("/bus/usb/devices"))
            .and_then(|()| ramfs.mkdev("/bus/usb/devices/list", dev::usb_devices_file()))
    };
    if let Err(error) = setup {
        error!("sysfs: cannot create USB view: {}", error);
        return;
    }

    ROOTFS.lock(|rootfs| {
        let rootfs = rootfs.as_mut().expect("root VFS not initialized");
        rootfs
            .mount_poinr
            .insert(MountPath("/sys/".to_string()), sysfs);
    });
}
