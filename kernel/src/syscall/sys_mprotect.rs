
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
    let inner = TASK_MANAER.task_que_inner.lock();
    let current = inner.current;
    drop(inner);

    let inner = TASK_MANAER.task_que_inner.lock();
    let mut tcb = inner.task_queen[current].lock();

    // 3. 委托 MapSet 执行核心逻辑（VMA 切割 + 标志更新 + PTE 同步）
    let result: isize = tcb.memory_set.mprotect_range(VirAddr(addr), len, prot);

    // 4. 成功返回前刷新 TLB，确保 CPU 使用新的权限位
    if result == 0 {
        // RISC-V: 全量 TLB 刷新
        // AArch64: 替换为 `tlbi vmalle1; dsb nsh`
        unsafe { core::arch::asm!("sfence.vma") };
        return 0;
    }

    // mprotect_range 返回具体 errno（EINVAL/ENOMEM），直接透传
    result
}
