//! DTB (Device Tree Blob) Parsing Module
//!
//! Reference: Linux 5.4.29
//! - drivers/of/fdt.c - FDT parsing core
//! - scripts/dtc/libfdt/fdt.h - FDT structure definitions
//! - include/linux/of.h - device_node structure
//!
//! ## Usage
//!
//! ```rust
//! // Initialize DTB parsing (call from kernel init)
//! driver::dtb::init();
//!
//! // Parse DTB from pointer
//! let parser = DtbParser::new(dtb_slice)?;
//! let tree = parser.parse()?;
//!
//! // Find devices
//! let scanner = DeviceScanner::new(&tree);
//! let cpus = scanner.find_cpus();
//! let memory = scanner.find_memory();
//! ```

mod devices;
mod fdt;
mod node;
mod parser;
mod probe;

pub use devices::DeviceScanner;
pub use node::{DeviceNode, DeviceTree};
pub use parser::DtbParser;
pub use probe::{ProbeEntry, ProbePriority};

use lazy_static::lazy_static;
use log::{debug, warn};

use crate::kprintln;
use crate::sync::UPSafeCell;

// Import DTB pointer from assembly (defined in entry.asm)
extern "C" {
    /// DTB pointer set by entry.asm (from x0 passed by bootloader)
    static _dtb_pointer: usize;
}

/// Maximum DTB size to map (10MB)
const DTB_MAX_SIZE: usize = 0xa00000;

lazy_static! {
    /// 缓存解析后的设备树。
    ///
    /// 设备树解析本身不依赖页帧分配器，但很多设备 probe 会依赖。
    /// 因此这里先缓存，等 frame allocator 初始化完成后再统一 probe。
    static ref PARSED_DEVICE_TREE: UPSafeCell<Option<DeviceTree>> = UPSafeCell::new(None);
}

/// Initialize DTB parsing
pub fn init() {
    crate::fs::vfs::clear_global_block_devices();

    // Get DTB pointer from assembly
    let dtb_ptr = unsafe { _dtb_pointer };

    if dtb_ptr == 0 {
        kprintln!("[DTB] No DTB pointer found");
        return;
    }

    kprintln!("[DTB] DTB pointer at {:#x}", dtb_ptr);

    // Create slice from DTB
    let dtb_slice = unsafe { core::slice::from_raw_parts(dtb_ptr as *const u8, DTB_MAX_SIZE) };

    match DtbParser::new(dtb_slice) {
        Ok(parser) => {
            match parser.parse() {
                Ok(tree) => {
                    // 扫描 memory 节点，注册到 KERNEL_MAIN_MEMORY
                    scan_and_register_memory(&tree);

                    // 日志输出设备树信息
                    trace_device_tree(&tree);
                    // 这里只做早期安全的解析和注册，不直接做设备 probe。
                    //
                    // 原因是块设备驱动可能在 probe 时申请连续页帧，
                    // 而 frame allocator 此时还没有初始化完成。
                    PARSED_DEVICE_TREE.lock(|dt| *dt = Some(tree));
                }
                Err(e) => {
                    warn!("[DTB] Failed to parse: {:?}", e);
                }
            }
        }
        Err(e) => {
            warn!("[DTB] Parse error: {:?}", e);
        }
    }
}

/// 返回 DTB header 中声明的 boot CPU 物理 ID。
///
/// 参考:
/// - `kernel/src/driver/dtb/fdt.rs:21-31,94-123`
pub fn boot_cpuid_phys() -> Option<u32> {
    let dtb_ptr = unsafe { _dtb_pointer };
    if dtb_ptr == 0 {
        return None;
    }

    let header_slice = unsafe { core::slice::from_raw_parts(dtb_ptr as *const u8, 40) };
    let header = fdt::FdtHeader::from_bytes(header_slice)?;
    header.validate().ok()?;
    Some(header.boot_cpuid_phys)
}

/// 扫描 DTB 中的 memory 节点，注册到 KERNEL_MAIN_MEMORY
fn scan_and_register_memory(tree: &DeviceTree) {
    use crate::memory::memorymodel::{finalize_main_memory, register_main_memory_region};

    let scanner = DeviceScanner::new(tree);
    let memory_regions = scanner.find_memory();

    if memory_regions.is_empty() {
        panic!("[DTB] 未找到 memory 节点");
        return;
    }

    for mem in &memory_regions {
        for reg in &mem.reg {
            if reg.size == 0 {
                continue;
            }
            let start = reg.address as usize;
            let end = start + reg.size as usize;
            register_main_memory_region(start, end);
        }
    }

    finalize_main_memory();
    debug!("[DTB] 物理内存注册完成");
}

/// 在 frame allocator 初始化完成后执行设备探测。
///
/// 这一阶段才允许驱动真正初始化 virtio 队列、申请连续页帧、
/// 注册全局块设备等依赖内存分配器的动作。
pub fn run_device_probes() {
    PARSED_DEVICE_TREE.lock(|tree_guard| {
        let Some(tree) = tree_guard.as_ref() else {
            warn!("[DTB] run_device_probes called before DTB init");
            return;
        };
        probe::run_probes(tree);
    });
}

/// Trace device tree content
fn trace_device_tree(tree: &DeviceTree) {
    // Root node info
    debug!("[DTB] Root Node: /");
    if let Some(model) = tree.root.get_string("model") {
        debug!("[DTB]   model: {}", model);
    }
    if let Some(compatible) = tree.root.get_string("compatible") {
        debug!("[DTB]   compatible: {}", compatible);
    }
    debug!("[DTB]   #address-cells: {}", tree.root.address_cells());
    debug!("[DTB]   #size-cells: {}", tree.root.size_cells());
    debug!("[DTB]");

    // Create scanner
    let scanner = DeviceScanner::new(tree);

    // CPUs
    let cpus = scanner.find_cpus();
    if !cpus.is_empty() {
        debug!("[DTB] === CPUs ===");
        for cpu in &cpus {
            debug!(
                "[DTB] {}: {}, reg={:#x}",
                cpu.name,
                cpu.compatible
                    .first()
                    .map(|s| s.as_str())
                    .unwrap_or("unknown"),
                cpu.reg
            );
        }
        debug!("[DTB]");
    }

    // Memory
    let memory = scanner.find_memory();
    if !memory.is_empty() {
        debug!("[DTB] === Memory ===");
        for mem in &memory {
            for reg in &mem.reg {
                let size_mb = reg.size / (1024 * 1024);
                debug!(
                    "[DTB] {}: {:#x}-{:#x} ({} MB)",
                    mem.name,
                    reg.address,
                    reg.address + reg.size,
                    size_mb
                );
            }
        }
        debug!("[DTB]");
    }

    // Interrupt Controllers
    let ics = scanner.find_interrupt_controllers();
    if !ics.is_empty() {
        debug!("[DTB] === Interrupt Controllers ===");
        for ic in &ics {
            debug!(
                "[DTB] {}: {}",
                ic.name,
                ic.compatible
                    .first()
                    .map(|s| s.as_str())
                    .unwrap_or("unknown")
            );
            for reg in &ic.reg {
                debug!(
                    "[DTB]   reg: {:#x}-{:#x}",
                    reg.address,
                    reg.address + reg.size
                );
            }
            debug!("[DTB]   #interrupt-cells: {}", ic.interrupt_cells);
        }
        debug!("[DTB]");
    }

    // GPIO Controllers
    let gpios = scanner.find_gpio_controllers();
    if !gpios.is_empty() {
        debug!("[DTB] === GPIO Controllers ===");
        for gpio in &gpios {
            debug!(
                "[DTB] {}: {}",
                gpio.name,
                gpio.compatible
                    .first()
                    .map(|s| s.as_str())
                    .unwrap_or("unknown")
            );
            for reg in &gpio.reg {
                debug!(
                    "[DTB]   reg: {:#x}-{:#x}",
                    reg.address,
                    reg.address + reg.size
                );
            }
        }
        debug!("[DTB]");
    }

    // Buses
    let buses = scanner.find_buses();
    if !buses.is_empty() {
        debug!("[DTB] === Buses ===");
        for bus in &buses {
            debug!("[DTB] {}: {:?}", bus.name, bus.bus_type);
        }
        debug!("[DTB]");
    }

    // MMIO Devices
    let devices = scanner.find_mmio_devices();
    if !devices.is_empty() {
        debug!("[DTB] === MMIO Devices ===");
        for dev in &devices {
            let compatible = dev
                .compatible
                .first()
                .map(|s| s.as_str())
                .unwrap_or("unknown");
            debug!("[DTB] {}: {}", dev.name, compatible);
            for reg in &dev.reg {
                debug!(
                    "[DTB]   reg: {:#x}-{:#x} ({} KB)",
                    reg.address,
                    reg.address + reg.size,
                    reg.size / 1024
                );
            }
        }
        debug!("[DTB]");
    }

    debug!(
        "[DTB] Total devices: {}",
        devices.len() + ics.len() + gpios.len() + buses.len()
    );
    debug!("[DTB] ================================");
}

/// DTB 设备探测宏
///
/// 在编译时注册设备探测器，支持自动设备发现和初始化。
///
/// # 参数
///
/// - `compatible`: 要匹配的 compatible 字符串
/// - `priority`: 探测优先级（High/Mid/Low）
/// - `driver`: 驱动名称（用于日志）
/// - `probe`: 探测回调函数
///
/// # 示例
///
/// ```rust
/// fn uart_probe(node: &DeviceNode, _compatible: &str) -> Result<(), &'static str> {
///     let regs = node.get_property("reg")
///         .ok_or("Missing reg")?
///         .as_reg(2, 2);
///     info!("Initializing UART at {:#x}", regs[0].address);
///     Ok(())
/// }
///
/// dtb_probe! {
///     compatible: "ns16550a",
///     priority: Mid,
///     driver: "uart-16550",
///     probe: uart_probe
/// }
/// ```
#[macro_export]
macro_rules! dtb_probe {
    (@emit $section:literal, $compatible:expr, $priority:ident, $driver:expr, $probe_fn:expr) => {
        const _: () = {
            #[used]
            #[link_section = $section]
            static DTB_PROBE_ENTRY: $crate::driver::dtb::ProbeEntry =
                $crate::driver::dtb::ProbeEntry::new(
                    $compatible,
                    $crate::driver::dtb::ProbePriority::$priority,
                    $probe_fn,
                    $driver,
                );
        };
    };
    (
        compatible: $compatible:expr,
        priority: High,
        driver: $driver:expr,
        probe: $probe_fn:expr
    ) => {
        $crate::dtb_probe!(@emit ".dtb_probe.high", $compatible, High, $driver, $probe_fn);
    };
    (
        compatible: $compatible:expr,
        priority: Mid,
        driver: $driver:expr,
        probe: $probe_fn:expr
    ) => {
        $crate::dtb_probe!(@emit ".dtb_probe.mid", $compatible, Mid, $driver, $probe_fn);
    };
    (
        compatible: $compatible:expr,
        priority: Low,
        driver: $driver:expr,
        probe: $probe_fn:expr
    ) => {
        $crate::dtb_probe!(@emit ".dtb_probe.low", $compatible, Low, $driver, $probe_fn);
    };
}
