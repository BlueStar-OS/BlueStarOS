mod task;
mod process;
use crate::fs::vfs::{OpenFlags, vfs_open};
use alloc::vec::{Vec};
use alloc::vec;
use log::{debug, error, info, warn};
/// 文件加载器，根据 app_id 从文件系统 /test 目录加载对应的 ELF 文件
/// app_id 从 0 开始
pub fn file_loader(file_path: &str) -> Vec<u8> {
    debug!("Eter in loader");
    let fd =match vfs_open(file_path, OpenFlags::RDONLY){
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