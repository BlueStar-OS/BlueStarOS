//! sys_tkill — 向指定线程发送信号。
//!
//! ## 作用
//! 直接按 TID 定位线程并投递信号。
//!
//! ## 参数
//! `tid` 为目标线程 ID；`sig` 为待发送信号。
//!
//! ## 注意事项
//! `tkill` 缺少 tgid 校验，Linux 已更推荐 `tgkill`；当前线程模型和信号投递尚未完整实现。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: `kernel/signal.c:4183`。
//!
//! ## 实现情况
//! 未实现；TODO: 补齐线程 ID 查找与 signal pending 语义后实现。

/// sys_tkill(tid, sig) -> 0 或 -errno。
///
/// 作用: 向特定线程发送信号。
/// 输入: `tid` 为线程 ID，`sig` 为信号编号。
/// 输出: 成功返回 0，失败返回负 errno。
/// 副作用: 完整实现时会修改目标线程 pending signal 集。
pub fn sys_tkill(tid: usize, sig: usize) -> isize {
    let _ = (tid, sig);
    // TODO: 缺失 Linux 强语义: TID 查找、线程级 pending signal 和权限检查。
    // 参考 K3 Linux 6.18.3 kernel/signal.c:4183。
    unimplemented!("sys_tkill: user TODO")
}
