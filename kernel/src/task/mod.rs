mod task;
mod process;
use crate::fs::vfs::{OpenFlags, vfs_open};
use alloc::vec::{Vec};
use alloc::vec;
use log::{debug, error, info, warn};
use bitflags::bitflags;

bitflags!{
    /// 信号结构体 posix
    pub struct Signal:usize{
        const SIGHUP    = 1usize << (1  - 1);  // 1  挂起（控制终端断开/会话结束）
        const SIGINT    = 1usize << (2  - 1);  // 2  中断（通常是 Ctrl+C）
        const SIGQUIT   = 1usize << (3  - 1);  // 3  退出（通常是 Ctrl+\\，可产生 core）
        const SIGILL    = 1usize << (4  - 1);  // 4  非法指令
        const SIGTRAP   = 1usize << (5  - 1);  // 5  断点/跟踪陷阱
        const SIGABRT   = 1usize << (6  - 1);  // 6  中止（abort）
        const SIGBUS    = 1usize << (7  - 1);  // 7  总线错误（对齐/物理访问错误等）
        const SIGFPE    = 1usize << (8  - 1);  // 8  浮点异常（除零/溢出等）
        const SIGKILL   = 1usize << (9  - 1);  // 9  强制杀死（不可捕获/不可忽略）
        const SIGUSR1   = 1usize << (10 - 1);  // 10 用户自定义信号 1
        const SIGSEGV   = 1usize << (11 - 1);  // 11 段错误（非法内存访问）
        const SIGUSR2   = 1usize << (12 - 1);  // 12 用户自定义信号 2
        const SIGPIPE   = 1usize << (13 - 1);  // 13 管道破裂（写入无读端的 pipe/socket）
        const SIGALRM   = 1usize << (14 - 1);  // 14 定时器超时（alarm）
        const SIGTERM   = 1usize << (15 - 1);  // 15 终止（默认终止，可捕获/可清理）
        const SIGSTKFLT = 1usize << (16 - 1);  // 16 协处理器栈故障（历史/很少用）
        const SIGCHLD   = 1usize << (17 - 1);  // 17 子进程状态改变（退出/停止/继续）
        const SIGCONT   = 1usize << (18 - 1);  // 18 继续（从停止状态恢复）
        const SIGSTOP   = 1usize << (19 - 1);  // 19 停止（不可捕获/不可忽略）
        const SIGTSTP   = 1usize << (20 - 1);  // 20 终端停止（Ctrl+Z）
        const SIGTTIN   = 1usize << (21 - 1);  // 21 后台进程读终端
        const SIGTTOU   = 1usize << (22 - 1);  // 22 后台进程写终端
        const SIGURG    = 1usize << (23 - 1);  // 23 紧急数据到达（socket）
        const SIGXCPU   = 1usize << (24 - 1);  // 24 CPU 时间限制超出
        const SIGXFSZ   = 1usize << (25 - 1);  // 25 文件大小限制超出
        const SIGVTALRM = 1usize << (26 - 1);  // 26 虚拟定时器超时
        const SIGPROF   = 1usize << (27 - 1);  // 27 性能分析定时器超时
        const SIGWINCH  = 1usize << (28 - 1);  // 28 窗口大小变化
        const SIGIO     = 1usize << (29 - 1);  // 29 I/O 就绪（异步 I/O）
        const SIGPWR    = 1usize << (30 - 1);  // 30 电源故障（历史/平台相关）
        const SIGSYS    = 1usize << (31 - 1);  // 31 错误的系统调用（bad syscall）
    }
}

/// 文件加载器，根据 app_id 从文件系统 /test 目录加载对应的 ELF 文件
/// app_id 从 0 开始
pub fn file_loader(file_path: &str) -> Vec<u8> {
    debug!("Eter in loader");
    let fd =match vfs_open(file_path, OpenFlags::empty()){
        Ok(res)=>{
            res
        }
        Err(_)=>{
            warn!("Open Application faild,can't open it");
            return vec![];//提前结束
        }
    };
    debug!("open file success");

    let mut out: Vec<u8> = Vec::new();
    let mut tmp = [0u8; 512];
    loop {
        let n = match fd.read(&mut tmp) {
            Ok(n) => n,
            Err(e) => {
                error!("file_loader: fd.read failed: path={} err={:?}", file_path, e);
                return vec![];
            }
        };
        if n == 0 {
            break;
        }
        out.extend_from_slice(&tmp[..n]);
    }
    info!("Load app for {} success!", file_path);
    out
}

pub use task::*;