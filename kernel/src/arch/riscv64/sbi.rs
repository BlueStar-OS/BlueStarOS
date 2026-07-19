use core::arch::asm;

use crate::fs::vfs::VfsFs;
use crate::root::ROOTFS;

const SET_TIMER: usize = 0;
const PUTC_CALLID: usize = 1;
const GETCHAR_CALLID: usize = 2;
const SHUTDOWN_CALLID: usize = 8;

/// 执行 SBI（Supervisor Binary Interface）调用。
///
/// 通过 `ecall` 指令陷入 M 模式，请求 OpenSBI 固件提供服务。
/// - `callid`: SBI 扩展 ID（EID）
/// - `fid`:   功能 ID（FID）
/// - `arg0`-`arg4`: 参数寄存器
/// - 返回: `a0` 中的返回值（`isize`）
#[inline(always)]
pub fn sbi_call(
    callid: usize,
    fid: usize,
    arg0: usize,
    arg1: usize,
    arg2: usize,
    arg3: usize,
    arg4: usize,
) -> isize {
    let mut result;
    unsafe {
        asm!(
            "ecall",
            inlateout("x10") arg0 => result,
            in("x11") arg1,
            in("x12") arg2,
            in("x13") arg3,
            in("x14") arg4,
            in("x16") fid,
            in("x17") callid,
        );
    }
    result
}
///关机的操作的副作用文件系统卸载
pub fn shutdown() -> ! {
    //取消文件系统挂载
    ROOTFS.try_lock(|lock_opt| {
        if let Some(rootfs_opt) = lock_opt {
            if let Some(rootfs) = rootfs_opt.as_ref() {
                rootfs.mount_poinr.iter().for_each(|fs| {
                    // 关机清理路径：umount 失败也无法回退，best-effort 忽略错误。
                    let _ = fs.1.lock().umount();
                });
            }
        }
    });

    sbi_call(SHUTDOWN_CALLID, 0, 0, 0, 0, 0, 0);

    loop {
        unsafe { asm!("wfi") };
    }
}
///设置下一次的时钟中断
pub fn set_next_timetriger(timer: usize) {
    sbi_call(SET_TIMER, 0, timer, 0, 0, 0, 0);
}

// ── SBI PMP 扩展 (EID=0x504D50) ─────────────────────────────────────────
// 运行时修改 PMP 条目权限, 用于给 S-mode 开放 APLIC/IMSIC MMIO 区域
//
// PMP flags 编码:
//   bit0=R  bit1=W  bit2=X  bit3=A(NAPOT=1)  bit7=L(lock)
//   开放 S/U R+W 用 flags=0xB (R|W|A)

const SBI_EXT_PMP: usize = 0x504D50;
const SBI_PMP_SET: usize = 0;
const SBI_PMP_GET: usize = 1;

/// 设置 PMP 条目 (NAPOT 模式)
/// addr: 物理基址, 需 NAPOT 对齐
/// log2size: NAPOT size = 2^order
/// flags: 权限 (0xB = S/U R+W, 0xD = S/U R+X, 0xF = S/U RWX)
/// 返回: 0=成功, 负值=错误
pub fn sbi_pmp_set(idx: usize, addr: usize, log2size: usize, flags: usize) -> isize {
    sbi_call(SBI_EXT_PMP, SBI_PMP_SET, idx, addr, log2size, flags, 0)
}

/// 读取 PMP 条目
/// 返回: PMP 配置值
pub fn sbi_pmp_get(idx: usize) -> isize {
    sbi_call(SBI_EXT_PMP, SBI_PMP_GET, idx, 0, 0, 0, 0)
}
