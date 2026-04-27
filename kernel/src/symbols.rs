//! 内核符号表：从 ELF 提取的函数地址→名称映射，用于 backtrace 解析。
//!
//! 符号文件 `kernel_symbols.txt` 由 Makefile 在 `cargo build` 后用 `rust-nm` 生成，
//! 每行格式：`000000008020xxxx funcname`，按地址升序排列。

/// 原始符号数据（编译期嵌入）
const SYMBOL_DATA: &str = include_str!("kernel_symbols.txt");

/// 内存中的符号条目
struct Sym {
    addr: usize,
    name: &'static str,
}

/// 懒初始化：首次使用时解析文本 → 静态切片
static mut SYMBOL_SLICE: Option<&'static [Sym]> = None;

/// 解析符号文本，返回静态符号数组。只在首次 backtrace 时调用一次。
fn parse_symbols() -> &'static [Sym] {
    unsafe {
        if let Some(ref syms) = SYMBOL_SLICE {
            return syms;
        }
    }

    let count = SYMBOL_DATA.lines().count();
    let mut vec: alloc::vec::Vec<Sym> = alloc::vec::Vec::with_capacity(count);

    for line in SYMBOL_DATA.lines() {
        if line.len() < 17 {
            continue;
        }
        // 格式: "000000008020xxxx name" — 前16个字符是十六进制地址，第17个是空格
        let addr_str = &line[..16];
        let name = line[17..].trim_end();
        if let Ok(addr) = usize::from_str_radix(addr_str, 16) {
            vec.push(Sym { addr, name });
        }
    }

    // 文本已按 nm -n 排序（地址升序），直接转为静态切片
    let slice = vec.leak();
    unsafe {
        SYMBOL_SLICE = Some(slice);
    }
    slice
}

/// 根据地址查找所属符号。
///
/// 返回 `(name, offset)` — 函数名 + 相对函数头的偏移。
/// 如果地址小于最小符号则返回 `None`。
pub fn lookup(addr: usize) -> Option<(&'static str, usize)> {
    let syms = parse_symbols();
    if syms.is_empty() {
        return None;
    }

    // 二分查找：找最后一个 addr <= target 的符号
    let mut lo = 0usize;
    let mut hi = syms.len();
    while lo < hi {
        let mid = (lo + hi) / 2;
        if syms[mid].addr <= addr {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }

    if lo == 0 {
        None
    } else {
        let s = &syms[lo - 1];
        let offset = addr - s.addr;
        Some((s.name, offset))
    }
}
