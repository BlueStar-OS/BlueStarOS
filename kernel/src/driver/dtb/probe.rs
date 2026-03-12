//! DTB 设备探测机制
//!
//! 提供编译时注册的设备探测器，支持自动设备发现和初始化。
//!
//! ## 使用示例
//!
//! ```rust
//! use crate::dtb_probe;
//!
//! fn uart_probe(node: &DeviceNode, _compatible: &str) -> Result<(), &'static str> {
//!     let regs = node.get_property("reg")
//!         .ok_or("Missing reg")?
//!         .as_reg(2, 2);
//!
//!     info!("Initializing UART at {:#x}", regs[0].address);
//!     Ok(())
//! }
//!
//! dtb_probe! {
//!     compatible: "ns16550a",
//!     priority: Mid,
//!     driver: "uart-16550",
//!     probe: uart_probe
//! }
//! ```

use crate::driver::dtb::{DeviceNode, DeviceTree};
use log::{info, warn, debug};

/// 探测优先级
///
/// 控制设备初始化顺序：
/// - High: 核心设备（中断控制器、时钟）
/// - Mid: 重要外设（UART、MMC）
/// - Low: 普通外设（GPIO、I2C）
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbePriority {
    Low = 0,
    Mid = 1,
    High = 2,
}

/// 探测回调函数类型
///
/// 参数：
/// - node: 匹配的设备节点
/// - compatible: 匹配的 compatible 字符串
///
/// 返回：
/// - Ok(()): 探测成功
/// - Err(msg): 探测失败，包含错误信息
pub type ProbeCallback = fn(node: &DeviceNode, compatible: &str) -> Result<(), &'static str>;

/// 探测条目（编译时注册）
#[repr(C)]
pub struct ProbeEntry {
    pub compatible: &'static str,
    pub priority: ProbePriority,
    pub callback: ProbeCallback,
    pub driver_name: &'static str,
}

impl ProbeEntry {
    /// 创建新的探测条目
    pub const fn new(
        compatible: &'static str,
        priority: ProbePriority,
        callback: ProbeCallback,
        driver_name: &'static str,
    ) -> Self {
        Self {
            compatible,
            priority,
            callback,
            driver_name,
        }
    }
}

// 从 linker 导入符号
extern "C" {
    static __dtb_probe_start: ProbeEntry;
    static __dtb_probe_end: ProbeEntry;
}

/// 获取所有注册的探测条目
pub fn get_probe_entries() -> &'static [ProbeEntry] {
    unsafe {
        let start = &__dtb_probe_start as *const ProbeEntry;
        let end = &__dtb_probe_end as *const ProbeEntry;
        let count = (end as usize - start as usize) / core::mem::size_of::<ProbeEntry>();
        core::slice::from_raw_parts(start, count)
    }
}

/// 执行设备探测
///
/// 遍历所有注册的探测器，查找匹配的设备节点并调用探测回调。
/// 探测器按优先级顺序执行（High → Mid → Low）。
pub fn run_probes(tree: &DeviceTree) {
    let entries = get_probe_entries();

    if entries.is_empty() {
        debug!("[DTB Probe] No probe entries registered");
        return;
    }

    info!("[DTB Probe] Found {} probe entries", entries.len());

    for entry in entries {
        // 查找匹配的节点
        let nodes = tree.find_compatible(entry.compatible);

        if nodes.is_empty() {
            continue;
        }

        debug!("[DTB Probe] Probing {} devices with compatible '{}'",
               nodes.len(), entry.compatible);

        for node in nodes {
            // 检查设备是否可用
            if !node.is_available() {
                debug!("[DTB Probe] Skipping disabled device: {}", node.full_name);
                continue;
            }

            // 调用探测回调
            match (entry.callback)(node, entry.compatible) {
                Ok(()) => {
                    info!("[DTB Probe] ✓ {} probed {} ({})",
                          entry.driver_name, node.full_name, entry.compatible);
                }
                Err(e) => {
                    warn!("[DTB Probe] ✗ {} failed to probe {}: {}",
                          entry.driver_name, node.full_name, e);
                }
            }
        }
    }

    info!("[DTB Probe] Probe completed");
}
