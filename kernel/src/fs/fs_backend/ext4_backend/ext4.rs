use crate::fs::vfs::*;
use alloc::string::ToString;
use rsext4::{Jbd2Dev, ext4_backend::ext4::Ext4FileSystem, fs_mount, fs_umount, mkfs};

use super::Ext4BlockDevice;

pub struct Ext4Fs {
    pub dev: Jbd2Dev<Ext4BlockDevice>,
    pub fs: Option<Ext4FileSystem>,
}

impl Ext4Fs {
    pub fn new(block_dev: Ext4BlockDevice) -> Self {
        let dev = Jbd2Dev::initial_jbd2dev(0, block_dev, false);
        Self { dev, fs: None }
    }
}

impl VfsFs for Ext4Fs {
    fn mount(&mut self) -> Result<(), VfsFsError> {
        if self.fs.is_some() {
            return Err(VfsFsError::Mounted);
        }
        let fs = fs_mount(&mut self.dev).map_err(|_| VfsFsError::MountFail)?;
        self.fs = Some(fs);
        Ok(())
    }

    fn name(&self) -> Result<alloc::string::String, VfsFsError> {
        Ok("ext4".to_string())
    }

    fn umount(&mut self) -> Result<(), VfsFsError> {
        let Some(fs) = self.fs.take() else {
            return Err(VfsFsError::Unmounted);
        };
        fs_umount(fs, &mut self.dev).map_err(|_| VfsFsError::UnmountFail)?;
        Ok(())
    }
}

pub fn new_ext4fs(block_dev: Ext4BlockDevice) -> Result<Ext4Fs, VfsFsError> {
    Ok(Ext4Fs::new(block_dev))
}
