//! sys_wait4 — 等待/回收子进程。
//!
//! ## 作用
//! 等待/回收子进程。
//!
//! ## 参数
//! `pid` 目标 pid；`wstatus_ptr` 状态写回；`options` 等待选项。
//!
//! ## 注意事项
//! 仅支持 pid=-1 或 pid>0；options 只处理 0/WNOHANG 风格。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: kernel/exit.c:1894
//!
//! ## 实现情况
//! 已实现基础路径。

use crate::syscall::syscall::*;
use crate::task::TaskStatus;

pub fn sys_wait4(pid: i32, wstatus_ptr: usize, options: i32) -> isize {
    let pid_isize = pid as isize;
    if pid_isize == 0 || pid_isize < -1 {
        warn!("sys_wait4: unsupport pid={}", pid_isize);
        return BlueErr::EINVAL.as_isize();
    }

    let target_pid: Option<i32> = if pid_isize == -1 { None } else { Some(pid) };

    loop {
        let children = TASK_MANAER.task_que_inner.lock(|inner| {
            if inner.task_queen.is_empty() {
                return Err(BlueErr::ECHILD);
            }
            let current = inner.current;
            let current_task = match inner.task_queen.get(current) {
                Some(t) => t.clone(),
                None => {
                    warn!(
                        "sys_wait4: current index out of bounds: current={} len={}",
                        current,
                        inner.task_queen.len()
                    );
                    return Err(BlueErr::ECHILD);
                }
            };
            Ok(current_task.lock(|t| t.childrens.clone()))
        });
        let children = match children {
            Ok(c) => c,
            Err(e) => return e.as_isize(),
        };

        if children.is_empty() {
            return BlueErr::ECHILD.as_isize();
        }

        if let Some(tp) = target_pid {
            let mut found = false;
            for child in children.iter() {
                let cpid = child.lock(|c| c.pid.0);
                if cpid == tp {
                    found = true;

                    let status = child.lock(|c| c.task_statut.clone());
                    if matches!(status, TaskStatus::Zombie) {
                        let exit_code = match TASK_MANAER.reap_zombie_child(cpid) {
                            Some(code) => code,
                            None => return BlueErr::ECHILD.as_isize(),
                        };
                        if wstatus_ptr != 0 {
                            let st: i32 = ((exit_code as i32) & 0xff) << 8;
                            let user_satp = TASK_MANAER.get_current_stap();
                            let mut slices = PageTable::get_mut_slice_from_satp(
                                user_satp,
                                size_of::<i32>(),
                                VirAddr(wstatus_ptr),
                            );
                            if slices.is_empty() {
                                return BlueErr::EFAULT.as_isize();
                            }
                            let bytes = st.to_le_bytes();
                            let mut written = 0usize;
                            for s in slices.iter_mut() {
                                let n =
                                    core::cmp::min(s.len(), bytes.len().saturating_sub(written));
                                if n == 0 {
                                    break;
                                }
                                s[..n].copy_from_slice(&bytes[written..written + n]);
                                written += n;
                            }
                            if written != bytes.len() {
                                return BlueErr::EFAULT.as_isize();
                            }
                        }
                        return cpid as isize;
                    }
                }
            }

            if !found {
                debug!("sys_wait4: target child not found pid={}", tp);
                return BlueErr::ECHILD.as_isize();
            }

            if options == 0 {
                warn!("Hang parent!");
                // TODO: 用户自行实现 — 通过 exit_queue 阻塞等待子进程退出
                let exit_q = TASK_MANAER
                    .task_que_inner
                    .lock(|inner| inner.task_queen[inner.current].lock(|t| t.exit_queue.clone()));
                exit_q.block();
                continue;
            }

            return 0;
        } else {
            for child in children.iter() {
                let cpid = child.lock(|c| c.pid.0);
                let status = child.lock(|c| c.task_statut.clone());
                if matches!(status, TaskStatus::Zombie) {
                    let exit_code = match TASK_MANAER.reap_zombie_child(cpid) {
                        Some(code) => code,
                        None => return BlueErr::ECHILD.as_isize(),
                    };
                    if wstatus_ptr != 0 {
                        let st: i32 = ((exit_code as i32) & 0xff) << 8;
                        let user_satp = TASK_MANAER.get_current_stap();
                        let mut slices = PageTable::get_mut_slice_from_satp(
                            user_satp,
                            size_of::<i32>(),
                            VirAddr(wstatus_ptr),
                        );
                        if slices.is_empty() {
                            return BlueErr::EFAULT.as_isize();
                        }
                        let bytes = st.to_le_bytes();
                        let mut written = 0usize;
                        for s in slices.iter_mut() {
                            let n = core::cmp::min(s.len(), bytes.len().saturating_sub(written));
                            if n == 0 {
                                break;
                            }
                            s[..n].copy_from_slice(&bytes[written..written + n]);
                            written += n;
                        }
                        if written != bytes.len() {
                            return BlueErr::EFAULT.as_isize();
                        }
                    }
                    return cpid as isize;
                }
            }

            // 多核可用这个，单核只能单任务
            // let has_child_run =children.iter().any(|cd|{
            //     cd.lock().task_statut == TaskStatus::Runing
            // });

            if options == 0 {
                //warn!("I'm, father,pid {} ",sys_getpid());
                // TODO: 用户自行实现 — 通过 exit_queue 阻塞等待子进程退出
                let exit_q = TASK_MANAER
                    .task_que_inner
                    .lock(|inner| inner.task_queen[inner.current].lock(|t| t.exit_queue.clone()));
                exit_q.block();

                // 父亲从这里苏醒 继续尝试回收
                continue;
                //warn!("I'm back! i'm father:{}",sys_getpid());
            }

            // 非阻塞，posix直接返回0
            return 0;
        }
        // 不需要轮询
        //TASK_MANAER.suspend_and_run_task();
    }
}
