mod process;
pub mod signal;
mod task;

use crate::fs::vfs::{vfs_open, OpenFlags};
use alloc::vec::Vec;
use bitflags::bitflags;
use log::{debug, error, warn};

bitflags! {
    /// POSIX 风格信号集合。
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Signal: usize {
        const SIGHUP    = 1usize << 0;
        const SIGINT    = 1usize << (2  - 1);
        const SIGQUIT   = 1usize << (3  - 1);
        const SIGILL    = 1usize << (4  - 1);
        const SIGTRAP   = 1usize << (5  - 1);
        const SIGABRT   = 1usize << (6  - 1);
        const SIGBUS    = 1usize << (7  - 1);
        const SIGFPE    = 1usize << (8  - 1);
        const SIGKILL   = 1usize << (9  - 1);
        const SIGUSR1   = 1usize << (10 - 1);
        const SIGSEGV   = 1usize << (11 - 1);
        const SIGUSR2   = 1usize << (12 - 1);
        const SIGPIPE   = 1usize << (13 - 1);
        const SIGALRM   = 1usize << (14 - 1);
        const SIGTERM   = 1usize << (15 - 1);
        const SIGSTKFLT = 1usize << (16 - 1);
        const SIGCHLD   = 1usize << (17 - 1);
        const SIGCONT   = 1usize << (18 - 1);
        const SIGSTOP   = 1usize << (19 - 1);
        const SIGTSTP   = 1usize << (20 - 1);
        const SIGTTIN   = 1usize << (21 - 1);
        const SIGTTOU   = 1usize << (22 - 1);
        const SIGURG    = 1usize << (23 - 1);
        const SIGXCPU   = 1usize << (24 - 1);
        const SIGXFSZ   = 1usize << (25 - 1);
        const SIGVTALRM = 1usize << (26 - 1);
        const SIGPROF   = 1usize << (27 - 1);
        const SIGWINCH  = 1usize << (28 - 1);
        const SIGIO     = 1usize << (29 - 1);
        const SIGPWR    = 1usize << (30 - 1);
        const SIGSYS    = 1usize << (31 - 1);
    }
}

/// 检查 ELF 魔数。
pub fn have_elf_header(data: [u8; 4]) -> bool {
    data == [0x7f, b'E', b'L', b'F']
}

/// 从 VFS 加载一个 ELF 文件。
pub fn file_loader(file_path: &str) -> Vec<u8> {
    debug!("Enter file loader: {}", file_path);

    let fd = match vfs_open(file_path, OpenFlags::empty()) {
        Ok(res) => res,
        Err(e) => {
            warn!("file_loader: open {} failed: {}", file_path, e);
            return Vec::new();
        }
    };

    let mut out = Vec::new();
    let mut tmp = [0u8; 512];
    let n = match fd.read(&mut tmp) {
        Ok(n) => n,
        Err(e) => {
            error!("file_loader: read {} failed: {:?}", file_path, e);
            return Vec::new();
        }
    };

    if n < 4 || !have_elf_header([tmp[0], tmp[1], tmp[2], tmp[3]]) {
        warn!("file_loader: {} is not a valid ELF file", file_path);
        return Vec::new();
    }

    out.extend_from_slice(&tmp[..n]);

    loop {
        let n = match fd.read(&mut tmp) {
            Ok(n) => n,
            Err(e) => {
                error!("file_loader: read {} failed: {:?}", file_path, e);
                return Vec::new();
            }
        };
        if n == 0 {
            break;
        }
        out.extend_from_slice(&tmp[..n]);
    }

    debug!("Loaded application {}", file_path);
    out
}

pub use task::*;
