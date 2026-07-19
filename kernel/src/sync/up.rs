use core::cell::{Cell, RefCell};

use log::error;

use crate::arch::{disable_irq, enable_irq};

/// 单核内部可变容器。
///
/// 包装 `RefCell`，在 `lock()` 失败时打印当前持有者的调用位置，
/// 帮助定位死锁和重入问题。
///
/// **中断安全**：`lock()` 和 `try_lock()` 在闭包执行期间关闭中断，
/// 闭包返回后恢复中断状态，防止中断上下文重入导致死锁。
///
/// **嵌套安全**：通过读取 `sstatus.SIE` 判断进入前中断是否已关闭。
/// 若已关闭（嵌套调用），内层 lock 返回时不恢复中断，由最外层恢复。
pub struct UPSafeCell<T> {
    inner: RefCell<T>,
    /// 当前持有者的调用位置 (文件名, 行号)。
    /// 用 `Cell` 而非 `RefCell`：元组是 `Copy`，无需运行时借用检查。
    borrower: Cell<(&'static str, u32)>,
}

unsafe impl<T> Sync for UPSafeCell<T> {}
unsafe impl<T> Send for UPSafeCell<T> {}

impl<T> UPSafeCell<T> {
    pub const fn new(value: T) -> Self {
        UPSafeCell {
            inner: RefCell::new(value),
            borrower: Cell::new(("", 0)),
        }
    }

    /// 获取可变借用并在闭包内操作，失败时打印持有者位置并 panic。
    ///
    /// `#[track_caller]` 让 `Location::caller()` 返回调用方的位置，
    /// 而非 `lock()` 内部的行号。
    ///
    /// 中断安全：闭包执行期间关闭中断，闭包返回后恢复。
    /// 嵌套安全：若进入前中断已关闭（嵌套），内层不恢复。
    #[track_caller]
    pub fn lock<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        let caller = core::panic::Location::caller();

        // 读取 sstatus.SIE，判断进入前中断是否已开启
        let sstatus: usize;
        unsafe { core::arch::asm!("csrr {}, sstatus", out(reg) sstatus) }
        let sie_was_on = (sstatus >> 1) & 1 == 1;
        if sie_was_on {
            disable_irq();
        }

        let result = match self.inner.try_borrow_mut() {
            Ok(mut g) => {
                self.borrower.set((caller.file(), caller.line()));
                f(&mut *g)
            }
            Err(_) => {
                if sie_was_on {
                    enable_irq();
                }
                let (file, line) = self.borrower.get();
                panic!(
                    "UPSafeCell: double borrow!\n  current: {}:{}\n  holder:  {}:{}",
                    caller.file(),
                    caller.line(),
                    file,
                    line,
                );
            }
        };

        // 仅当进入前中断是开启的才恢复，嵌套调用不会破坏外层的关中断状态
        if sie_was_on {
            enable_irq();
        }
        result
    }

    /// 非阻塞尝试获取借用。
    ///
    /// 中断安全：闭包执行期间关闭中断，闭包返回后恢复。
    /// 嵌套安全：若进入前中断已关闭（嵌套），内层不恢复。
    #[track_caller]
    pub fn try_lock<R>(&self, f: impl FnOnce(Option<&mut T>) -> R) -> R {
        let sstatus: usize;
        unsafe { core::arch::asm!("csrr {}, sstatus", out(reg) sstatus) }
        let sie_was_on = (sstatus >> 1) & 1 == 1;
        if sie_was_on {
            disable_irq();
        }

        let result = match self.inner.try_borrow_mut() {
            Ok(mut g) => {
                let caller = core::panic::Location::caller();
                self.borrower.set((caller.file(), caller.line()));
                f(Some(&mut *g))
            }
            Err(_) => {
                let caller = core::panic::Location::caller();
                let (file, line) = self.borrower.get();
                error!(
                    "UPSafeCell try_lock failed!\n  current: {}:{}\n  holder:  {}:{}",
                    caller.file(),
                    caller.line(),
                    file,
                    line,
                );
                f(None)
            }
        };

        if sie_was_on {
            enable_irq();
        }
        result
    }
}
