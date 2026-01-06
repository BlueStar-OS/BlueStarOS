use alloc::collections::btree_map::BTreeMap;
use alloc::string::String;
use alloc::string::ToString;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::any::Any;
use spin::Mutex;

use crate::fs::vfs::{
    File, LinuxDirent64, MountFs, OpenFlags, VfsFs, VfsFsError, VfsStat, VFS_DT_DIR, VFS_DT_REG,
};

const ROOT_INODE: u32 = 1;

#[derive(Clone)]
struct NodeMeta {
    inode: u32,
}

enum NodeKind {
    Dir { entries: BTreeMap<String, u32> },
    File { data: Vec<u8> },
    Device { file: Arc<dyn File> },
}

struct Node {
    meta: NodeMeta,
    kind: NodeKind,
}

pub struct RamFs {
    mounted: bool,
    max_bytes: usize,
    used_bytes: usize,
    next_inode: u32,
    nodes: BTreeMap<u32, Node>,
}

impl RamFs {
    pub fn new(max_bytes: usize) -> Self {
        let mut nodes = BTreeMap::new();
        nodes.insert(
            ROOT_INODE,
            Node {
                meta: NodeMeta { inode: ROOT_INODE },
                kind: NodeKind::Dir {
                    entries: BTreeMap::new(),
                },
            },
        );
        Self {
            mounted: false,
            max_bytes,
            used_bytes: 0,
            next_inode: ROOT_INODE + 1,
            nodes,
        }
    }

    fn alloc_inode(&mut self) -> u32 {
        let ino = self.next_inode;
        self.next_inode = self.next_inode.wrapping_add(1);
        ino
    }

    fn normalize_components(path: &str) -> Result<Vec<&str>, VfsFsError> {
        if path.is_empty() {
            return Err(VfsFsError::Invalid);
        }
        if !path.starts_with('/') {
            return Err(VfsFsError::Invalid);
        }
        let mut out = Vec::new();
        for comp in path.split('/') {
            if comp.is_empty() {
                continue;
            }
            if comp == "." {
                continue;
            }
            if comp == ".." {
                return Err(VfsFsError::Invalid);
            }
            out.push(comp);
        }
        Ok(out)
    }

    fn lookup_path(&self, path: &str) -> Result<u32, VfsFsError> {
        let comps = Self::normalize_components(path)?;
        let mut cur = ROOT_INODE;
        for c in comps {
            let node = self.nodes.get(&cur).ok_or(VfsFsError::NotFound)?;
            let NodeKind::Dir { entries } = &node.kind else {
                return Err(VfsFsError::NotDir);
            };
            cur = *entries.get(c).ok_or(VfsFsError::NotFound)?;
        }
        Ok(cur)
    }

    fn split_parent(&self, path: &str) -> Result<(u32, String), VfsFsError> {
        if path == "/" {
            return Err(VfsFsError::Invalid);
        }
        let comps = Self::normalize_components(path)?;
        if comps.is_empty() {
            return Err(VfsFsError::Invalid);
        }
        let name = comps[comps.len() - 1].to_string();
        let mut cur = ROOT_INODE;
        for c in &comps[..comps.len() - 1] {
            let node = self.nodes.get(&cur).ok_or(VfsFsError::NotFound)?;
            let NodeKind::Dir { entries } = &node.kind else {
                return Err(VfsFsError::NotDir);
            };
            cur = *entries.get(*c).ok_or(VfsFsError::NotFound)?;
        }
        Ok((cur, name))
    }

    fn create_node(&mut self, parent: u32, name: &str, kind: NodeKind) -> Result<u32, VfsFsError> {
        let ino = self.alloc_inode();
        let parent_node = self.nodes.get_mut(&parent).ok_or(VfsFsError::NotFound)?;
        let NodeKind::Dir { entries } = &mut parent_node.kind else {
            return Err(VfsFsError::NotDir);
        };
        if entries.contains_key(name) {
            return Err(VfsFsError::AlreadyExists);
        }
        entries.insert(name.to_string(), ino);
        self.nodes.insert(
            ino,
            Node {
                meta: NodeMeta { inode: ino },
                kind,
            },
        );
        Ok(ino)
    }

    pub fn mkdev(&mut self, path: &str, file: Arc<dyn File>) -> Result<(), VfsFsError> {
        let (parent_ino, name) = self.split_parent(path)?;
        let _ = self.create_node(parent_ino, &name, NodeKind::Device { file })?;
        Ok(())
    }

    fn file_read_at(&self, ino: u32, off: usize, buf: &mut [u8]) -> Result<usize, VfsFsError> {
        let node = self.nodes.get(&ino).ok_or(VfsFsError::NotFound)?;
        let NodeKind::File { data } = &node.kind else {
            return Err(VfsFsError::IsDir);
        };
        if off >= data.len() {
            return Ok(0);
        }
        let end = core::cmp::min(data.len(), off + buf.len());
        let n = end - off;
        buf[..n].copy_from_slice(&data[off..end]);
        Ok(n)
    }

    fn file_write_at(&mut self, ino: u32, off: usize, buf: &[u8]) -> Result<usize, VfsFsError> {
        let node = self.nodes.get_mut(&ino).ok_or(VfsFsError::NotFound)?;
        let NodeKind::File { data } = &mut node.kind else {
            return Err(VfsFsError::IsDir);
        };
        let required = off.saturating_add(buf.len());
        if required > data.len() {
            let grow = required - data.len();
            if self.used_bytes.saturating_add(grow) > self.max_bytes {
                return Err(VfsFsError::NoSpace);
            }
            data.resize(required, 0);
            self.used_bytes = self.used_bytes.saturating_add(grow);
        }
        data[off..off + buf.len()].copy_from_slice(buf);
        Ok(buf.len())
    }

    fn file_truncate(&mut self, ino: u32, new_len: usize) -> Result<(), VfsFsError> {
        let node = self.nodes.get_mut(&ino).ok_or(VfsFsError::NotFound)?;
        let NodeKind::File { data } = &mut node.kind else {
            return Err(VfsFsError::IsDir);
        };
        if new_len > data.len() {
            let grow = new_len - data.len();
            if self.used_bytes.saturating_add(grow) > self.max_bytes {
                return Err(VfsFsError::NoSpace);
            }
            data.resize(new_len, 0);
            self.used_bytes = self.used_bytes.saturating_add(grow);
        } else {
            let shrink = data.len() - new_len;
            data.truncate(new_len);
            self.used_bytes = self.used_bytes.saturating_sub(shrink);
        }
        Ok(())
    }

    fn stat_inode(&self, ino: u32) -> Result<VfsStat, VfsFsError> {
        let node = self.nodes.get(&ino).ok_or(VfsFsError::NotFound)?;
        match &node.kind {
            NodeKind::Dir { .. } => Ok(VfsStat {
                inode: node.meta.inode,
                size: 0,
                mode: 0,
                file_type: VFS_DT_DIR,
            }),
            NodeKind::File { data } => Ok(VfsStat {
                inode: node.meta.inode,
                size: data.len() as u64,
                mode: 0,
                file_type: VFS_DT_REG,
            }),
            NodeKind::Device { file } => {
                let mut st = file.stat()?;
                st.inode = node.meta.inode;
                Ok(st)
            }
        }
    }

    fn getdents_stream(&self, ino: u32) -> Result<Vec<u8>, VfsFsError> {
        let node = self.nodes.get(&ino).ok_or(VfsFsError::NotFound)?;
        let NodeKind::Dir { entries } = &node.kind else {
            return Err(VfsFsError::NotDir);
        };

        let mut stream: Vec<u8> = Vec::new();
        let mut cur_off: u64 = 0;
        let hdr_len = core::mem::size_of::<LinuxDirent64>();

        for (name, child_ino) in entries.iter() {
            let child = self.nodes.get(child_ino).ok_or(VfsFsError::NotFound)?;
            let dtype = match child.kind {
                NodeKind::Dir { .. } => VFS_DT_DIR,
                NodeKind::File { .. } | NodeKind::Device { .. } => VFS_DT_REG,
            };

            let name_bytes = name.as_bytes();
            let reclen = align_up(hdr_len + name_bytes.len() + 1, 8);
            let base = stream.len();
            stream.resize(base + reclen, 0);
            cur_off = cur_off.saturating_add(reclen as u64);

            let hdr = LinuxDirent64 {
                d_ino: *child_ino as u64,
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
            stream[name_base..name_base + name_bytes.len()].copy_from_slice(name_bytes);
            stream[name_base + name_bytes.len()] = 0;
        }

        Ok(stream)
    }
}

fn align_up(x: usize, a: usize) -> usize {
    (x + a - 1) / a * a
}

pub struct RamFile {
    mount_fs: MountFs,
    inode: u32,
    offset: Mutex<u64>,
    flags: OpenFlags,
}

impl RamFile {
    fn with_fs_mut<T>(&self, f: impl FnOnce(&mut RamFs) -> Result<T, VfsFsError>) -> Result<T, VfsFsError> {
        let mut guard = self.mount_fs.lock();
        let fs = guard
            .as_any_mut()
            .downcast_mut::<RamFs>()
            .ok_or(VfsFsError::IO)?;
        f(fs)
    }

    fn with_fs<T>(&self, f: impl FnOnce(&RamFs) -> Result<T, VfsFsError>) -> Result<T, VfsFsError> {
        let guard = self.mount_fs.lock();
        let fs = guard.as_any().downcast_ref::<RamFs>().ok_or(VfsFsError::IO)?;
        f(fs)
    }
}

impl File for RamFile {
    fn read(&self, buf: &mut [u8]) -> Result<usize, VfsFsError> {
        if !self.flags.read {
            return Err(VfsFsError::PermissionDenied);
        }
        let off = *self.offset.lock() as usize;
        let n = self.with_fs(|fs| fs.file_read_at(self.inode, off, buf))?;
        *self.offset.lock() = (off + n) as u64;
        Ok(n)
    }

    fn write(&self, buf: &[u8]) -> Result<usize, VfsFsError> {
        if !self.flags.write {
            return Err(VfsFsError::PermissionDenied);
        }
        let mut off = *self.offset.lock() as usize;
        if self.flags.append {
            let end = self.with_fs(|fs| {
                let st = fs.stat_inode(self.inode)?;
                Ok(st.size as usize)
            })?;
            off = end;
            *self.offset.lock() = off as u64;
        }
        let n = self.with_fs_mut(|fs| fs.file_write_at(self.inode, off, buf))?;
        *self.offset.lock() = (off + n) as u64;
        Ok(n)
    }

    fn lseek(&self, offset: isize, whence: usize) -> Result<usize, VfsFsError> {
        let cur = *self.offset.lock() as i64;
        let end = self.with_fs(|fs| {
            let st = fs.stat_inode(self.inode)?;
            Ok(st.size as i64)
        })?;

        let next = match whence {
            0 => offset as i64,
            1 => cur.saturating_add(offset as i64),
            2 => end.saturating_add(offset as i64),
            _ => return Err(VfsFsError::Invalid),
        };
        if next < 0 {
            return Err(VfsFsError::Invalid);
        }
        *self.offset.lock() = next as u64;
        Ok(next as usize)
    }

    fn getdents64(&self, max_len: usize) -> Result<Vec<u8>, VfsFsError> {
        if max_len == 0 {
            return Ok(Vec::new());
        }

        let stream = self.with_fs(|fs| fs.getdents_stream(self.inode))?;
        let off = *self.offset.lock() as usize;
        if off >= stream.len() {
            *self.offset.lock() = stream.len() as u64;
            return Ok(Vec::new());
        }
        let end = core::cmp::min(stream.len(), off + max_len);
        let out = stream[off..end].to_vec();
        *self.offset.lock() = end as u64;
        Ok(out)
    }

    fn stat(&self) -> Result<VfsStat, VfsFsError> {
        self.with_fs(|fs| fs.stat_inode(self.inode))
    }

    fn flush(&self) -> Result<(), VfsFsError> {
        Ok(())
    }
}

impl VfsFs for RamFs {
    fn mount(&mut self) -> Result<(), VfsFsError> {
        if self.mounted {
            return Err(VfsFsError::Mounted);
        }
        self.mounted = true;
        Ok(())
    }

    fn umount(&mut self) -> Result<(), VfsFsError> {
        if !self.mounted {
            return Err(VfsFsError::Unmounted);
        }
        self.mounted = false;
        Ok(())
    }

    fn name(&self) -> Result<String, VfsFsError> {
        Ok("ramfs".to_string())
    }

    fn mkdir(&mut self, path: &str) -> Result<(), VfsFsError> {
        let (parent, name) = self.split_parent(path)?;
        let _ = self.create_node(parent, &name, NodeKind::Dir { entries: BTreeMap::new() })?;
        Ok(())
    }

    fn mkfile(&mut self, path: &str) -> Result<(), VfsFsError> {
        let (parent_ino, name) = self.split_parent(path)?;
        let _ = self.create_node(parent_ino, &name, NodeKind::File { data: Vec::new() })?;
        Ok(())
    }

    fn open(&mut self, mount_fs: MountFs, path: &str, flags: OpenFlags) -> Result<Arc<dyn File>, VfsFsError> {
        let ino = match self.lookup_path(path) {
            Ok(ino) => ino,
            Err(VfsFsError::NotFound) if flags.create => {
                self.mkfile(path)?;
                self.lookup_path(path)?
            }
            Err(e) => return Err(e),
        };

        let node = self.nodes.get(&ino).ok_or(VfsFsError::NotFound)?;
        match &node.kind {
            NodeKind::Dir { .. } => Ok(Arc::new(RamFile {
                mount_fs,
                inode: ino,
                offset: Mutex::new(0),
                flags,
            })),
            NodeKind::File { .. } => {
                if flags.truncate {
                    if !flags.write {
                        return Err(VfsFsError::PermissionDenied);
                    }
                    self.file_truncate(ino, 0)?;
                }
                Ok(Arc::new(RamFile {
                    mount_fs,
                    inode: ino,
                    offset: Mutex::new(0),
                    flags,
                }))
            }
            NodeKind::Device { file } => {
                if flags.truncate {
                    return Err(VfsFsError::NotSupported);
                }
                Ok(file.clone())
            }
        }
    }

    fn truncate(&mut self, path: &str, size: u64) -> Result<(), VfsFsError> {
        let ino = self.lookup_path(path)?;
        let node = self.nodes.get(&ino).ok_or(VfsFsError::NotFound)?;
        match node.kind {
            NodeKind::File { .. } => self.file_truncate(ino, size as usize),
            NodeKind::Dir { .. } => Err(VfsFsError::IsDir),
            NodeKind::Device { .. } => Err(VfsFsError::NotSupported),
        }
    }

    fn unlink(&mut self, path: &str) -> Result<(), VfsFsError> {
        let (parent, name) = self.split_parent(path)?;
        let parent_node = self.nodes.get_mut(&parent).ok_or(VfsFsError::NotFound)?;
        let NodeKind::Dir { entries } = &mut parent_node.kind else {
            return Err(VfsFsError::NotDir);
        };
        let child = entries.remove(&name).ok_or(VfsFsError::NotFound)?;
        let node = self.nodes.get(&child).ok_or(VfsFsError::NotFound)?;
        if matches!(node.kind, NodeKind::Dir { .. }) {
            return Err(VfsFsError::IsDir);
        }
        if let Some(Node { kind, .. }) = self.nodes.remove(&child) {
            if let NodeKind::File { data } = kind {
                self.used_bytes = self.used_bytes.saturating_sub(data.len());
            }
        }
        Ok(())
    }

    fn stat(&mut self, path: &str) -> Result<VfsStat, VfsFsError> {
        let ino = self.lookup_path(path)?;
        self.stat_inode(ino)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
