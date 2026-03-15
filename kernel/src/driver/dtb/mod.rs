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

mod fdt;
mod node;
mod parser;
mod devices;
mod probe;

pub use fdt::{FdtHeader, FdtError, MemReservation, FDT_MAGIC};
pub use node::{DeviceNode, DeviceTree, Property, RegBlock, AddressRange};
pub use parser::DtbParser;
pub use devices::{
    CpuInfo, MemoryInfo, MmioDevice, InterruptController,
    GpioController, ClockController, BusBridge, BusType,
    DeviceScanner,
};
pub use probe::{ProbeEntry, ProbePriority, ProbeCallback};

use log::{debug, trace, info, warn};

// Import DTB pointer from assembly (defined in entry.asm)
extern "C" {
    /// DTB pointer set by entry.asm (from x0 passed by bootloader)
    static _dtb_pointer: usize;
}

/// Maximum DTB size to map (1MB)
const DTB_MAX_SIZE: usize = 0x100000;

/// Initialize DTB parsing
pub fn init() {
    // Get DTB pointer from assembly
    let dtb_ptr = unsafe { _dtb_pointer };

    if dtb_ptr == 0 {
        warn!("[DTB] No DTB pointer found");
        return;
    }

    debug!("[DTB] DTB pointer at {:#x}", dtb_ptr);

    // Create slice from DTB
    let dtb_slice = unsafe {
        core::slice::from_raw_parts(dtb_ptr as *const u8, DTB_MAX_SIZE)
    };

    match DtbParser::new(dtb_slice) {
        Ok(parser) => {
            match parser.parse() {
                Ok(tree) => {
                    // 日志输出设备树信息
                    trace_device_tree(&tree);
                    // 执行设备探测（GIC/UART 等注册 MMIO）
                    probe::run_probes(&tree);
                    // 扫描 memory 节点，注册到 KERNEL_MAIN_MEMORY
                    scan_and_register_memory(&tree);
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

/// 扫描 DTB 中的 memory 节点，注册到 KERNEL_MAIN_MEMORY
fn scan_and_register_memory(tree: &DeviceTree) {
    use crate::memory::memorymodel::{register_main_memory_region, finalize_main_memory};

    let scanner = DeviceScanner::new(tree);
    let memory_regions = scanner.find_memory();

    if memory_regions.is_empty() {
        warn!("[DTB] 未找到 memory 节点");
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
    info!("[DTB] 物理内存注册完成");
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
            debug!("[DTB] {}: {}, reg={:#x}",
                   cpu.name,
                   cpu.compatible.first().map(|s| s.as_str()).unwrap_or("unknown"),
                   cpu.reg);
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
               debug!("[DTB] {}: {:#x}-{:#x} ({} MB)",
                       mem.name, reg.address, reg.address + reg.size, size_mb);
            }
        }
        debug!("[DTB]");
    }

    // Interrupt Controllers
    let ics = scanner.find_interrupt_controllers();
    if !ics.is_empty() {
        debug!("[DTB] === Interrupt Controllers ===");
        for ic in &ics {
            debug!("[DTB] {}: {}",
                   ic.name,
                   ic.compatible.first().map(|s| s.as_str()).unwrap_or("unknown"));
            for reg in &ic.reg {
                debug!("[DTB]   reg: {:#x}-{:#x}",
                       reg.address, reg.address + reg.size);
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
            debug!("[DTB] {}: {}",
                   gpio.name,
                   gpio.compatible.first().map(|s| s.as_str()).unwrap_or("unknown"));
            for reg in &gpio.reg {
                debug!("[DTB]   reg: {:#x}-{:#x}",
                       reg.address, reg.address + reg.size);
            }
        }
        debug!("[DTB]");
    }

    // Buses
    let buses = scanner.find_buses();
    if !buses.is_empty() {
        debug!("[DTB] === Buses ===");
        for bus in &buses {
            debug!("[DTB] {}: {:?}",
                   bus.name, bus.bus_type);
        }
        debug!("[DTB]");
    }

    // MMIO Devices
    let devices = scanner.find_mmio_devices();
    if !devices.is_empty() {
        debug!("[DTB] === MMIO Devices ===");
        for dev in &devices {
            let compatible = dev.compatible.first()
                .map(|s| s.as_str()).unwrap_or("unknown");
            debug!("[DTB] {}: {}", dev.name, compatible);
            for reg in &dev.reg {
                debug!("[DTB]   reg: {:#x}-{:#x} ({} KB)",
                       reg.address, reg.address + reg.size, reg.size / 1024);
            }
        }
        debug!("[DTB]");
    }

    debug!("[DTB] Total devices: {}", devices.len() + ics.len() + gpios.len() + buses.len());
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
    (
        compatible: $compatible:expr,
        priority: $priority:ident,
        driver: $driver:expr,
        probe: $probe_fn:expr
    ) => {
        $crate::paste::paste! {
            #[used]
            #[link_section = concat!(".dtb_probe.", stringify!([<$priority:lower>]))]
            static [<DTB_PROBE_ $priority:upper _ $driver:upper>]:
                $crate::driver::dtb::ProbeEntry =
                $crate::driver::dtb::ProbeEntry::new(
                    $compatible,
                    $crate::driver::dtb::ProbePriority::$priority,
                    $probe_fn,
                    $driver,
                );
        }
    };
}