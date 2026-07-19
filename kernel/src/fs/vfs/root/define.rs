use crate::alloc::string::ToString;
use crate::fs::vfs::vfs::VfsFs;
use crate::fs::vfs::VfsFsError;
use crate::sync::UPSafeCell;
use alloc::collections::btree_map::BTreeMap;
use alloc::{string::String, sync::Arc};
use lazy_static::lazy_static;
use spin::Mutex;

lazy_static! {
    /// 全局根文件系统。
    pub static ref ROOTFS: UPSafeCell<Option<RootFs>> = UPSafeCell::new(None);
}

/// 挂载点路径。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MountPath(pub String);

impl PartialOrd for MountPath {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for MountPath {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        let self_deep = self.0.chars().filter(|c| *c == '/').count();
        let other_deep = other.0.chars().filter(|c| *c == '/').count();
        if self_deep > other_deep {
            return core::cmp::Ordering::Less;
        } else if self_deep < other_deep {
            return core::cmp::Ordering::Greater;
        }
        self.0.cmp(&other.0)
    }
}

// 全局虚拟文件系统。
pub struct RootFs {
    pub mount_poinr: BTreeMap<MountPath, Arc<Mutex<dyn VfsFs>>>,
}

/// 挂载点解析结果：命中的文件系统句柄 + 该 FS 视角下的剩余相对路径。
type MountResolved = Option<(Arc<Mutex<dyn VfsFs>>, String)>;

/// 挂载点匹配过程中的最优候选：(匹配得分/前缀长度, 文件系统句柄, 剩余路径)。
type MountCandidate = Option<(usize, Arc<Mutex<dyn VfsFs>>, String)>;

impl RootFs {
    fn normalize_abs_path(path: &str) -> String {
        let mut out = String::new();
        let mut prev_slash = false;
        for ch in path.chars() {
            if ch == '/' {
                if !prev_slash {
                    out.push('/');
                }
                prev_slash = true;
            } else {
                out.push(ch);
                prev_slash = false;
            }
        }
        if out.is_empty() {
            out.push('/');
        }
        while out.len() > 1 && out.ends_with('/') {
            out.pop();
        }
        out
    }

    fn is_component_prefix(mount: &str, path: &str) -> bool {
        let mount = if mount.len() > 1 {
            mount.trim_end_matches('/')
        } else {
            mount
        };

        if mount == "/" {
            return path.starts_with('/');
        }
        if path == mount {
            return true;
        }
        if path.starts_with(mount) {
            return path.as_bytes().get(mount.len()) == Some(&b'/');
        }
        false
    }

    /// 解析挂载点和剩余路径。
    pub fn resolve_mount_point(
        &self,
        path: &str,
    ) -> Result<MountResolved, VfsFsError> {
        let abs = Self::normalize_abs_path(path);

        let mut best: MountCandidate = None;
        for (mp, fs) in self.mount_poinr.iter() {
            let mps = Self::normalize_abs_path(mp.0.as_str());
            if !Self::is_component_prefix(&mps, abs.as_str()) {
                continue;
            }

            let sub = if mps == "/" {
                abs.clone()
            } else if abs.len() == mps.len() {
                "/".to_string()
            } else {
                abs[mps.len()..].to_string()
            };

            let score = mps.len();
            match &best {
                Some((best_score, _, _)) if *best_score >= score => {}
                _ => best = Some((score, fs.clone(), sub)),
            }
        }

        Ok(best.map(|(_, fs, sub)| (fs, sub)))
    }
}
