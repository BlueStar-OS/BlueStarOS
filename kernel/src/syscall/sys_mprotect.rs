//! sys_mprotect — 修改既有用户映射的访问权限。
//!
//! ## 作用
//! 调整当前进程 `[addr, addr + len)` 虚拟地址区间的 PTE/VMA 访问权限。
//!
//! ## 参数
//! `addr` 为页对齐起始虚拟地址；`len` 为保护范围长度；`prot` 为 PROT_READ/PROT_WRITE/PROT_EXEC 权限组合。
//!
//! ## 注意事项
//! 权限更新后必须刷新 TLB；当前只覆盖本地 `sfence.vma`，SMP 下还需要跨核 shootdown。
//!
//! ## Linux 参考版本
//! K3 Linux 6.18.3 (/home/inkbottle/othersrc/k3/spacemit-k3-linux-6.18)，参考: mm/mprotect.c:1008。
//!
//! ## 实现情况
//! 已实现页对齐校验、权限校验、VMA/PTE 更新和本地 TLB 刷新；TODO: 补齐 Linux 的 mmap_lock、SMP TLB shootdown 和细粒度 VMA flag 兼容。
//!
use crate::arch::memory::VirAddr;
use crate::error::BlueErr::EINVAL;
use crate::memory::MmapProt;
use crate::task::TASK_MANAER;
use crate::PAGE_SIZE;
/// mprotect 系统调用：修改 [addr, addr+len) 地址范围的访问权限
///
/// ## 参数
/// - `addr`: 起始虚拟地址，**必须页对齐**
/// - `len`: 保护范围长度（字节），向上取整到页边界
/// - `prot`: 新权限位掩码（PROT_READ=1, PROT_WRITE=2, PROT_EXEC=4）
///
/// ## 返回值
/// - `0`: 成功
/// - `-EINVAL`: addr 未对齐 / len=0 / prot 包含无效位
/// - `-ENOMEM`: [addr, addr+len) 范围中存在未被任何 VMA 覆盖的空洞
///
/// ## 实现流程
/// 1. 数据清洗：验证输入对齐、长度和权限位
/// 2. 数据查找：遍历进程的 VMA 列表，检查目标范围是否完整覆盖
/// 3. 数据重构：对部分覆盖的 VMA 执行切割，分离出受影响区间
/// 4. 状态转移：更新受影响 VMA 的 MapAreaFlags
/// 5. 硬件同步：逐页更新 PTE 中的 R/W/X 位，保留 V/A/D/U/DEV 不变
/// 6. TLB 刷新：执行 sfence.vma 强制 CPU 放弃旧权限缓存
pub fn sys_mprotect(addr: usize, len: usize, prot: usize) -> isize {
    // 1. 数据清洗：addr 必须按页对齐，len 不能为零
    if !addr.is_multiple_of(PAGE_SIZE) || len == 0 {
        return -EINVAL;
    }

    // prot 权限位检查：只允许 READ/WRITE/EXEC 及其合法组合
    if MmapProt::from_bits(prot).is_none() {
        return -EINVAL;
    }

    // 2. 获取当前进程的地址空间
    let result = TASK_MANAER.task_que_inner.lock(|inner| {
        let current = inner.current;
        inner.task_queen[current]
            .lock(|tcb| tcb.memory_set.mprotect_range(VirAddr(addr), len, prot))
    });

    // 3. 成功返回前刷新 TLB
    if result == 0 {
        // SAFETY: sfence.vma 是 RISC-V 特权指令，用于使当前 hart 的旧页表权限缓存失效；
        // 此处只在内核态执行，且不读写 Rust 管理的内存对象。
        unsafe { core::arch::asm!("sfence.vma") };
        return 0;
    }

    result
}
