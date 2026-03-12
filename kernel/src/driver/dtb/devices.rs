//! Device-specific parsing
//! Reference: Linux 5.4.29 drivers/of/

use alloc::string::String;
use alloc::vec::Vec;

use super::node::{DeviceNode, Property, RegBlock};

/// CPU Information
#[derive(Debug, Clone)]
pub struct CpuInfo {
    pub name: String,
    pub device_type: String,
    pub compatible: Vec<String>,
    pub reg: u32,
    pub enable_method: Option<String>,
    pub clock_frequency: Option<u32>,
}

impl CpuInfo {
    /// Parse from device node
    pub fn from_node(node: &DeviceNode) -> Option<Self> {
        // Check if this is a CPU node
        if node.get_string("device_type")? != "cpu" {
            return None;
        }

        let reg = node.get_u32("reg")?;
        let compatible = node.compatible();
        let enable_method = node.get_string("enable-method");
        let clock_frequency = node.get_u32("clock-frequency");

        Some(Self {
            name: node.full_name.clone(),
            device_type: String::from("cpu"),
            compatible,
            reg,
            enable_method,
            clock_frequency,
        })
    }
}

/// Memory Information
#[derive(Debug, Clone)]
pub struct MemoryInfo {
    pub name: String,
    pub device_type: String,
    pub reg: Vec<RegBlock>,
}

impl MemoryInfo {
    /// Parse from device node
    pub fn from_node(node: &DeviceNode, addr_cells: u32, size_cells: u32) -> Option<Self> {
        // Check if this is a memory node
        if node.get_string("device_type")? != "memory" {
            return None;
        }

        let prop = node.get_property("reg")?;
        let reg = prop.as_reg(addr_cells, size_cells);

        Some(Self {
            name: node.full_name.clone(),
            device_type: String::from("memory"),
            reg,
        })
    }
}

/// MMIO Device
#[derive(Debug, Clone)]
pub struct MmioDevice {
    pub name: String,
    pub compatible: Vec<String>,
    pub reg: Vec<RegBlock>,
    pub interrupts: Option<Vec<u32>>,
    pub interrupt_parent: Option<u32>,
    pub status: String,
}

impl MmioDevice {
    /// Parse from device node
    pub fn from_node(node: &DeviceNode, addr_cells: u32, size_cells: u32) -> Option<Self> {
        // Must have reg property
        let reg_prop = node.get_property("reg")?;
        let reg = reg_prop.as_reg(addr_cells, size_cells);

        if reg.is_empty() {
            return None;
        }

        let compatible = node.compatible();
        if compatible.is_empty() {
            return None;
        }

        let interrupts = node.get_property("interrupts").map(|p| p.as_u32_list());
        let interrupt_parent = node.get_u32("interrupt-parent");
        let status = node.get_string("status").unwrap_or_else(|| String::from("okay"));

        Some(Self {
            name: node.full_name.clone(),
            compatible,
            reg,
            interrupts,
            interrupt_parent,
            status,
        })
    }
}

/// Interrupt Controller
#[derive(Debug, Clone)]
pub struct InterruptController {
    pub name: String,
    pub compatible: Vec<String>,
    pub reg: Vec<RegBlock>,
    pub interrupt_cells: u32,
    pub phandle: Option<u32>,
}

impl InterruptController {
    /// Parse from device node
    pub fn from_node(node: &DeviceNode, addr_cells: u32, size_cells: u32) -> Option<Self> {
        // Must have interrupt-controller property
        if node.get_property("interrupt-controller").is_none() {
            return None;
        }

        let compatible = node.compatible();
        if compatible.is_empty() {
            return None;
        }

        let reg = node.get_property("reg")
            .map(|p| p.as_reg(addr_cells, size_cells))
            .unwrap_or_default();

        let interrupt_cells = node.get_u32("#interrupt-cells").unwrap_or(0);
        let phandle = node.phandle;

        Some(Self {
            name: node.full_name.clone(),
            compatible,
            reg,
            interrupt_cells,
            phandle,
        })
    }
}

/// GPIO Controller
#[derive(Debug, Clone)]
pub struct GpioController {
    pub name: String,
    pub compatible: Vec<String>,
    pub reg: Vec<RegBlock>,
    pub gpio_cells: u32,
    pub ngpios: Option<u32>,
    pub phandle: Option<u32>,
}

impl GpioController {
    /// Parse from device node
    pub fn from_node(node: &DeviceNode, addr_cells: u32, size_cells: u32) -> Option<Self> {
        // Must have gpio-controller property
        if node.get_property("gpio-controller").is_none() {
            return None;
        }

        let compatible = node.compatible();
        if compatible.is_empty() {
            return None;
        }

        let reg = node.get_property("reg")
            .map(|p| p.as_reg(addr_cells, size_cells))
            .unwrap_or_default();

        let gpio_cells = node.get_u32("#gpio-cells").unwrap_or(0);
        let ngpios = node.get_u32("ngpios");
        let phandle = node.phandle;

        Some(Self {
            name: node.full_name.clone(),
            compatible,
            reg,
            gpio_cells,
            ngpios,
            phandle,
        })
    }
}

/// Clock Controller
#[derive(Debug, Clone)]
pub struct ClockController {
    pub name: String,
    pub compatible: Vec<String>,
    pub reg: Vec<RegBlock>,
    pub clock_cells: u32,
    pub phandle: Option<u32>,
}

impl ClockController {
    /// Parse from device node
    pub fn from_node(node: &DeviceNode, addr_cells: u32, size_cells: u32) -> Option<Self> {
        // Must have #clock-cells property
        let clock_cells = node.get_u32("#clock-cells")?;

        let compatible = node.compatible();
        if compatible.is_empty() {
            return None;
        }

        let reg = node.get_property("reg")
            .map(|p| p.as_reg(addr_cells, size_cells))
            .unwrap_or_default();

        let phandle = node.phandle;

        Some(Self {
            name: node.full_name.clone(),
            compatible,
            reg,
            clock_cells,
            phandle,
        })
    }
}

/// Bus Type
#[derive(Debug, Clone)]
pub enum BusType {
    Pci,
    Usb,
    I2c,
    Spi,
    Axi,
    Ahb,
    Apb,
    Vci,
    Other(String),
}

impl BusType {
    fn from_compatible(compatible: &[String]) -> Self {
        for c in compatible {
            if c.contains("pci") || c.contains("pcie") {
                return Self::Pci;
            }
            if c.contains("usb") {
                return Self::Usb;
            }
            if c.contains("i2c") {
                return Self::I2c;
            }
            if c.contains("spi") {
                return Self::Spi;
            }
            if c.contains("axi") {
                return Self::Axi;
            }
            if c.contains("ahb") {
                return Self::Ahb;
            }
            if c.contains("apb") {
                return Self::Apb;
            }
            if c.contains("vci") {
                return Self::Vci;
            }
        }
        Self::Other(String::new())
    }
}

/// Bus/Bridge
#[derive(Debug, Clone)]
pub struct BusBridge {
    pub name: String,
    pub compatible: Vec<String>,
    pub reg: Vec<RegBlock>,
    pub bus_type: BusType,
    pub address_cells: u32,
    pub size_cells: u32,
}

impl BusBridge {
    /// Parse from device node
    pub fn from_node(node: &DeviceNode, addr_cells: u32, size_cells: u32) -> Option<Self> {
        // Check if this is a bus node (has ranges or #address-cells)
        if node.get_property("ranges").is_none() && node.get_property("#address-cells").is_none() {
            return None;
        }

        let compatible = node.compatible();
        let bus_type = BusType::from_compatible(&compatible);

        let reg = node.get_property("reg")
            .map(|p| p.as_reg(addr_cells, size_cells))
            .unwrap_or_default();

        let node_addr_cells = node.get_u32("#address-cells").unwrap_or(addr_cells);
        let node_size_cells = node.get_u32("#size-cells").unwrap_or(size_cells);

        Some(Self {
            name: node.full_name.clone(),
            compatible,
            reg,
            bus_type,
            address_cells: node_addr_cells,
            size_cells: node_size_cells,
        })
    }
}

/// Device tree scanner
pub struct DeviceScanner<'a> {
    tree: &'a super::node::DeviceTree,
}

impl<'a> DeviceScanner<'a> {
    pub fn new(tree: &'a super::node::DeviceTree) -> Self {
        Self { tree }
    }

    /// Get root address cells and size cells
    pub fn root_cells(&self) -> (u32, u32) {
        (self.tree.root.address_cells(), self.tree.root.size_cells())
    }

    /// Find all CPUs
    pub fn find_cpus(&self) -> Vec<CpuInfo> {
        let mut result = Vec::new();
        self.find_cpus_recursive(&self.tree.root, &mut result);
        result
    }

    fn find_cpus_recursive(&self, node: &DeviceNode, result: &mut Vec<CpuInfo>) {
        if let Some(cpu) = CpuInfo::from_node(node) {
            result.push(cpu);
        }
        for child in &node.children {
            self.find_cpus_recursive(child, result);
        }
    }

    /// Find all memory regions
    pub fn find_memory(&self) -> Vec<MemoryInfo> {
        let (addr_cells, size_cells) = self.root_cells();
        let mut result = Vec::new();
        self.find_memory_recursive(&self.tree.root, addr_cells, size_cells, &mut result);
        result
    }

    fn find_memory_recursive(&self, node: &DeviceNode, addr_cells: u32, size_cells: u32, result: &mut Vec<MemoryInfo>) {
        if let Some(mem) = MemoryInfo::from_node(node, addr_cells, size_cells) {
            result.push(mem);
        }
        // 递归到子节点时，使用当前节点的 cells 值（父节点的 cells 用于解析子节点的 reg）
        let node_addr = node.address_cells();
        let node_size = node.size_cells();
        for child in &node.children {
            self.find_memory_recursive(child, node_addr, node_size, result);
        }
    }

    /// Find all interrupt controllers
    pub fn find_interrupt_controllers(&self) -> Vec<InterruptController> {
        let (addr_cells, size_cells) = self.root_cells();
        let mut result = Vec::new();
        self.find_ic_recursive(&self.tree.root, addr_cells, size_cells, &mut result);
        result
    }

    fn find_ic_recursive(&self, node: &DeviceNode, addr_cells: u32, size_cells: u32, result: &mut Vec<InterruptController>) {
        if let Some(ic) = InterruptController::from_node(node, addr_cells, size_cells) {
            result.push(ic);
        }
        let node_addr = node.address_cells();
        let node_size = node.size_cells();
        for child in &node.children {
            self.find_ic_recursive(child, node_addr, node_size, result);
        }
    }

    /// Find all GPIO controllers
    pub fn find_gpio_controllers(&self) -> Vec<GpioController> {
        let (addr_cells, size_cells) = self.root_cells();
        let mut result = Vec::new();
        self.find_gpio_recursive(&self.tree.root, addr_cells, size_cells, &mut result);
        result
    }

    fn find_gpio_recursive(&self, node: &DeviceNode, addr_cells: u32, size_cells: u32, result: &mut Vec<GpioController>) {
        if let Some(gpio) = GpioController::from_node(node, addr_cells, size_cells) {
            result.push(gpio);
        }
        let node_addr = node.address_cells();
        let node_size = node.size_cells();
        for child in &node.children {
            self.find_gpio_recursive(child, node_addr, node_size, result);
        }
    }

    /// Find all MMIO devices (UART, MMC, Ethernet, etc.)
    pub fn find_mmio_devices(&self) -> Vec<MmioDevice> {
        let (addr_cells, size_cells) = self.root_cells();
        let mut result = Vec::new();
        self.find_mmio_recursive(&self.tree.root, addr_cells, size_cells, &mut result);
        result
    }

    fn find_mmio_recursive(&self, node: &DeviceNode, addr_cells: u32, size_cells: u32, result: &mut Vec<MmioDevice>) {
        // Skip memory, cpu, interrupt-controller, gpio-controller nodes
        if node.get_string("device_type") == Some(String::from("memory"))
            || node.get_string("device_type") == Some(String::from("cpu"))
            || node.get_property("interrupt-controller").is_some()
            || node.get_property("gpio-controller").is_some() {
            // Still recurse into children
            let node_addr = node.address_cells();
            let node_size = node.size_cells();
            for child in &node.children {
                self.find_mmio_recursive(child, node_addr, node_size, result);
            }
            return;
        }

        if let Some(dev) = MmioDevice::from_node(node, addr_cells, size_cells) {
            result.push(dev);
        }

        let node_addr = node.address_cells();
        let node_size = node.size_cells();
        for child in &node.children {
            self.find_mmio_recursive(child, node_addr, node_size, result);
        }
    }

    /// Find all buses
    pub fn find_buses(&self) -> Vec<BusBridge> {
        let (addr_cells, size_cells) = self.root_cells();
        let mut result = Vec::new();
        self.find_bus_recursive(&self.tree.root, addr_cells, size_cells, &mut result);
        result
    }

    fn find_bus_recursive(&self, node: &DeviceNode, addr_cells: u32, size_cells: u32, result: &mut Vec<BusBridge>) {
        if let Some(bus) = BusBridge::from_node(node, addr_cells, size_cells) {
            result.push(bus);
        }
        let node_addr = node.address_cells();
        let node_size = node.size_cells();
        for child in &node.children {
            self.find_bus_recursive(child, node_addr, node_size, result);
        }
    }
}