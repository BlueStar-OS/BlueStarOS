//! sys_execve — 用 ELF 镜像替换当前进程地址空间。
//!
//! ## 作用
//! 用 ELF 镜像替换当前进程地址空间。
//!
//! ## 参数
//! `path_ptr` 路径；`argv_ptr` argv 指针数组；`envp_ptr` envp 指针数组。
//!
//! ## 注意事项
//! envp 当前忽略；argv 有 MAX_ARGC 限制。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: fs/exec.c:2005
//!
//! ## 实现情况
//! 已实现基础路径。

use crate::syscall::syscall::*;
use crate::task::file_loader;
use alloc::string::String;
use alloc::vec::Vec;

pub fn sys_execve(path_ptr: usize, argv_ptr: usize, envp_ptr: usize) -> isize {
    const MAX_ARGC: usize = 256;

    // 关键：先拿到 satp，再去 lock 当前 task，避免二次借用。
    let user_satp = TASK_MANAER.get_current_stap();

    debug!(
        "sys_execve: path_ptr={:#x} argv_ptr={:#x} envp_ptr={:#x} satp={:#x}",
        path_ptr, argv_ptr, envp_ptr, user_satp
    );

    if envp_ptr != 0 {
        warn!(
            "sys_execve: envp is ignored for now envp_ptr={:#x}",
            envp_ptr
        );
    }

    let path = match read_c_string_from_user_with_satp(user_satp, path_ptr) {
        Ok(p) => p,
        Err(_) => return BlueErr::EFAULT.as_isize(),
    };

    let elf_data = file_loader(&path);
    if elf_data.is_empty() {
        return BlueErr::ENOENT.as_isize();
    }

    // 读取 argv 指针数组（NULL 结尾）
    let mut exec_argv: Vec<String> = Vec::new();
    if argv_ptr != 0 {
        for i in 0..MAX_ARGC {
            let elem_ptr = argv_ptr + i * core::mem::size_of::<usize>();
            let mut slices = PageTable::get_mut_slice_from_satp(
                user_satp,
                core::mem::size_of::<usize>(),
                VirAddr(elem_ptr),
            );
            if slices.is_empty() {
                error!("sys_execve: invalid argv element addr={:#x}", elem_ptr);
                return BlueErr::EFAULT.as_isize();
            }
            let mut flat: Vec<u8> = Vec::with_capacity(core::mem::size_of::<usize>());
            for s in slices.iter_mut() {
                flat.extend_from_slice(s);
            }
            if flat.len() < core::mem::size_of::<usize>() {
                error!("sys_execve: short read argv element addr={:#x}", elem_ptr);
                return BlueErr::EINVAL.as_isize();
            }
            let ptr_bytes: [u8; core::mem::size_of::<usize>()] =
                flat[..core::mem::size_of::<usize>()].try_into().unwrap();
            let cptr = usize::from_ne_bytes(ptr_bytes);
            if cptr == 0 {
                break;
            }
            match read_c_string_from_user_with_satp(user_satp, cptr) {
                Ok(s) => exec_argv.push(s),
                Err(e) => {
                    error!(
                        "sys_execve: Can't translate argv[{}] ptr={:#x} err={}",
                        i, cptr, e
                    );
                    return BlueErr::EFAULT.as_isize();
                }
            }
        }
    }

    let argc = exec_argv.len();
    let current_task = TASK_MANAER
        .task_que_inner
        .lock(|inner| inner.task_queen[inner.current].clone());
    current_task.lock(|tcb| {
        if !tcb.new_exec_task_with_elf(&path, exec_argv, argc, &elf_data) {
            return BlueErr::ENOEXEC.as_isize();
        }
        0
    })
}
