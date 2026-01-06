use crate::fs::vfs::*;
use alloc::string::ToString;
use rsext4::{Jbd2Dev, ext4_backend::ext4::Ext4FileSystem, fs_mount, fs_umount, mkfs};

use alloc::format;
use alloc::sync::Arc;
use rsext4::{
    OpenFile,
    lseek as ext4_lseek,
    mkdir as ext4_mkdir,
    mkfile as ext4_mkfile,
    mv as ext4_mv,
    open as ext4_open,
    read_at as ext4_read_at,
    rename as ext4_rename,
    truncate as ext4_truncate,
    write_at as ext4_write_at,
};
use rsext4::ext4_backend::dir::get_inode_with_num;
use rsext4::ext4_backend::entries::DirEntryIterator;
use rsext4::ext4_backend::file::unlink as ext4_unlink;
use rsext4::ext4_backend::loopfile::resolve_inode_block_allextend;
use rsext4::ext4_backend::config::BLOCK_SIZE;
use alloc::vec::Vec;
use super::Ext4BlockDevice;

pub struct Ext4Fs {
    pub dev: Jbd2Dev<Ext4BlockDevice>,
    pub fs: Option<Ext4FileSystem>,
}

fn align_up(x: usize, align: usize) -> usize {
    (x + align - 1) & !(align - 1)
}

pub struct Ext4File {
    mount: MountFs,
    of: spin::Mutex<OpenFile>,
}

impl Ext4File {
    pub fn new(mount: MountFs, of: OpenFile) -> Self {
        Self {
            mount,
            of: spin::Mutex::new(of),
        }
    }

    fn with_ext4_mut<T>(&self, f: impl FnOnce(&mut Ext4Fs) -> Result<T, VfsFsError>) -> Result<T, VfsFsError> {
        let mut guard = self.mount.lock();
        let ext4 = guard
            .as_any_mut()
            .downcast_mut::<Ext4Fs>()
            .ok_or(VfsFsError::NotSupported)?;
        f(ext4)
    }
}

impl File for Ext4File {
    fn read(&self, buf: &mut [u8]) -> Result<usize, VfsFsError> {
        let data = self.with_ext4_mut(|ext4| {
            let fs_inner = ext4.fs.as_mut().ok_or(VfsFsError::IO)?;
            let mut of = self.of.lock();
            ext4_read_at(&mut ext4.dev, fs_inner, &mut *of, buf.len()).map_err(|_| VfsFsError::IO)
        })?;
        let n = core::cmp::min(buf.len(), data.len());
        buf[..n].copy_from_slice(&data[..n]);
        Ok(n)
    }

    fn write(&self, buf: &[u8]) -> Result<usize, VfsFsError> {
        self.with_ext4_mut(|ext4| {
            let fs_inner = ext4.fs.as_mut().ok_or(VfsFsError::IO)?;
            let mut of = self.of.lock();
            ext4_write_at(&mut ext4.dev, fs_inner, &mut *of, buf).map_err(|_| VfsFsError::IO)?;
            Ok(buf.len())
        })
    }

    fn read_at(&self, offset: usize, buf: &mut [u8]) -> Result<usize, VfsFsError> {
        let data = self.with_ext4_mut(|ext4| {
            let fs_inner = ext4.fs.as_mut().ok_or(VfsFsError::IO)?;
            let mut of = self.of.lock();
            ext4_lseek(&mut *of, offset as u64);
            ext4_read_at(&mut ext4.dev, fs_inner, &mut *of, buf.len()).map_err(|_| VfsFsError::IO)
        })?;
        let n = core::cmp::min(buf.len(), data.len());
        buf[..n].copy_from_slice(&data[..n]);
        Ok(n)
    }

    fn write_at(&self, offset: usize, buf: &[u8]) -> Result<usize, VfsFsError> {
        self.with_ext4_mut(|ext4| {
            let fs_inner = ext4.fs.as_mut().ok_or(VfsFsError::IO)?;
            let mut of = self.of.lock();
            ext4_lseek(&mut *of, offset as u64);
            ext4_write_at(&mut ext4.dev, fs_inner, &mut *of, buf).map_err(|_| VfsFsError::IO)?;
            Ok(buf.len())
        })
    }

    fn lseek(&self, offset: isize, whence: usize) -> Result<usize, VfsFsError> {
        let mut of = self.of.lock();
        let cur = of.offset as i64;
        let off = offset as i64;
        let new_off = match whence {
            0 => off,
            1 => cur.saturating_add(off),
            2 => {
                let end = of.inode.size() as i64;
                end.saturating_add(off)
            }
            _ => return Err(VfsFsError::NotSupported),
        };
        if new_off < 0 {
            return Err(VfsFsError::NotSupported);
        }
        ext4_lseek(&mut *of, new_off as u64);
        Ok(of.offset as usize)
    }

    fn stat(&self) -> Result<VfsStat, VfsFsError> {
        let of = self.of.lock();
        let file_type = if of.inode.is_dir() {
            VFS_DT_DIR
        } else if of.inode.is_file() {
            VFS_DT_REG
        } else {
            VFS_DT_UNKNOWN
        };
        Ok(VfsStat {
            inode: of.inode_num,
            size: of.inode.size(),
            mode: 0,
            file_type,
        })
    }

    fn getdents64(&self, max_len: usize) -> Result<Vec<u8>, VfsFsError> {
        if max_len == 0 {
            return Ok(Vec::new());
        }

        let (sub_dir, stream_off) = {
            let of = self.of.lock();
            (of.path.clone(), of.offset as usize)
        };

        let blocks = self.with_ext4_mut(|ext4| {
            let fs_inner = ext4.fs.as_mut().ok_or(VfsFsError::IO)?;
            let got = get_inode_with_num(fs_inner, &mut ext4.dev, &sub_dir)
                .map_err(|_| VfsFsError::IO)?
                .ok_or(VfsFsError::NotFound)?;
            let (_dir_ino, mut dir_inode) = got;
            if !dir_inode.is_dir() {
                return Err(VfsFsError::NotDir);
            }
            let blocks = resolve_inode_block_allextend(fs_inner, &mut ext4.dev, &mut dir_inode)
                .map_err(|_| VfsFsError::IO)?;
            Ok(blocks)
        })?;

        let mut stream: Vec<u8> = Vec::new();
        let mut cur_off: u64 = 0;
        let hdr_len = core::mem::size_of::<LinuxDirent64>();

        for &phys in blocks.values() {
            let data: Vec<u8> = self.with_ext4_mut(|ext4| {
                let fs_inner = ext4.fs.as_mut().ok_or(VfsFsError::IO)?;
                let cached = fs_inner
                    .datablock_cache
                    .get_or_load(&mut ext4.dev, phys)
                    .map_err(|_| VfsFsError::IO)?;
                Ok(cached.data[..BLOCK_SIZE].to_vec())
            })?;
            let data = &data[..];
            let iter = DirEntryIterator::new(data);
            for (entry, _) in iter {
                if entry.inode == 0 {
                    continue;
                }

                if entry.name == b"." || entry.name == b".." {
                    continue;
                }

                let dtype = match entry.file_type {
                    1 => VFS_DT_REG,
                    2 => VFS_DT_DIR,
                    7 => VFS_DT_LNK,
                    _ => VFS_DT_UNKNOWN,
                };

                let name_len = entry.name.len();
                let reclen = align_up(hdr_len + name_len + 1, 8);

                let base = stream.len();
                stream.resize(base + reclen, 0);
                cur_off = cur_off.saturating_add(reclen as u64);

                let hdr = LinuxDirent64 {
                    d_ino: entry.inode as u64,
                    d_off: cur_off,
                    d_reclen: reclen as u16,
                    d_type: dtype as u8,
                };
                let hdr_bytes: &[u8] = unsafe {
                    core::slice::from_raw_parts(
                        (&hdr as *const LinuxDirent64) as *const u8,
                        core::mem::size_of::<LinuxDirent64>(),
                    )
                };
                stream[base..base + hdr_bytes.len()].copy_from_slice(hdr_bytes);

                let name_base = base + hdr_len;
                stream[name_base..name_base + name_len].copy_from_slice(entry.name);
                stream[name_base + name_len] = 0;
            }
        }

        if stream_off >= stream.len() {
            let mut of = self.of.lock();
            of.offset = stream.len() as u64;
            return Ok(Vec::new());
        }

        let end = core::cmp::min(stream.len(), stream_off + max_len);
        let out = stream[stream_off..end].to_vec();
        let mut of = self.of.lock();
        of.offset = end as u64;
        Ok(out)
    }
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

    fn open(&mut self, mount_fs: MountFs, path: &str, flags: OpenFlags) -> Result<Arc<dyn File>, VfsFsError> {
        let fs_inner = self.fs.as_mut().ok_or(VfsFsError::IO)?;
        let mut of = ext4_open(&mut self.dev, fs_inner, path, flags.create).map_err(|_| VfsFsError::IO)?;
        if flags.append {
            let end = of.inode.size() as u64;
            ext4_lseek(&mut of, end);
        }
        if flags.truncate {
            if flags.write {
                ext4_truncate(&mut self.dev, fs_inner, path, 0).map_err(|_| VfsFsError::IO)?;
            } else {
                return Err(VfsFsError::PermissionDenied);
            }
        }
        Ok(Arc::new(Ext4File::new(mount_fs, of)))
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

    fn mkdir(&mut self, path: &str) -> Result<(), VfsFsError> {
        let fs_inner = self.fs.as_mut().ok_or(VfsFsError::IO)?;
        let res = ext4_mkdir(&mut self.dev, fs_inner, path);
        if res.is_none() {
            return Err(VfsFsError::IO);
        }
        Ok(())
    }

    fn mkfile(&mut self, path: &str) -> Result<(), VfsFsError> {
        let fs_inner = self.fs.as_mut().ok_or(VfsFsError::IO)?;
        let res = ext4_mkfile(&mut self.dev, fs_inner, path, None, None);
        if res.is_none() {
            return Err(VfsFsError::IO);
        }
        Ok(())
    }

    fn mv(&mut self, src: &str, dest: &str) -> Result<(), VfsFsError> {
        let fs_inner = self.fs.as_mut().ok_or(VfsFsError::IO)?;
        ext4_mv(fs_inner, &mut self.dev, src, dest).map_err(|_| VfsFsError::IO)
    }

    fn rename(&mut self, path: &str, new_name: &str) -> Result<(), VfsFsError> {
        let fs_inner = self.fs.as_mut().ok_or(VfsFsError::IO)?;
        let new_path = if let Some(pos) = path.rfind('/') {
            let parent = &path[..pos];
            if parent.is_empty() {
                format!("/{new_name}")
            } else {
                format!("{parent}/{new_name}")
            }
        } else {
            new_name.to_string()
        };
        ext4_rename(&mut self.dev, fs_inner, path, &new_path).map_err(|_| VfsFsError::IO)
    }

    fn truncate(&mut self, path: &str, size: u64) -> Result<(), VfsFsError> {
        let fs_inner = self.fs.as_mut().ok_or(VfsFsError::IO)?;
        ext4_truncate(&mut self.dev, fs_inner, path, size).map_err(|_| VfsFsError::IO)
    }

    fn unlink(&mut self, path: &str) -> Result<(), VfsFsError> {
        let fs_inner = self.fs.as_mut().ok_or(VfsFsError::IO)?;
        let got = get_inode_with_num(fs_inner, &mut self.dev, path)
            .map_err(|_| VfsFsError::IO)?
            .ok_or(VfsFsError::NotFound)?;
        let (_ino, inode) = got;
        if inode.is_dir() {
            return Err(VfsFsError::IsDir);
        }
        ext4_unlink(fs_inner, &mut self.dev, path);
        Ok(())
    }

    fn stat(&mut self, path: &str) -> Result<VfsStat, VfsFsError> {
        let fs_inner = self.fs.as_mut().ok_or(VfsFsError::IO)?;
        let got = get_inode_with_num(fs_inner, &mut self.dev, path)
            .map_err(|_| VfsFsError::IO)?
            .ok_or(VfsFsError::NotFound)?;
        let (ino, inode) = got;
        let file_type = if inode.is_dir() {
            VFS_DT_DIR
        } else if inode.is_file() {
            VFS_DT_REG
        } else {
            VFS_DT_UNKNOWN
        };
        Ok(VfsStat {
            inode: ino,
            size: inode.size(),
            mode: 0,
            file_type,
        })
    }

    fn as_any(&self) -> &dyn core::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn core::any::Any {
        self
    }
}

pub fn new_ext4fs(block_dev: Ext4BlockDevice) -> Result<Ext4Fs, VfsFsError> {
    Ok(Ext4Fs::new(block_dev))
}
