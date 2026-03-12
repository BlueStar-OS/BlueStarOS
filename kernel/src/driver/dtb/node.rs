//! Device Tree Node structures
//! Reference: Linux 5.4.29 include/linux/of.h

use alloc::string::String;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;

/// Register block for reg property
#[derive(Debug, Clone, Copy)]
pub struct RegBlock {
    pub address: u64,
    pub size: u64,
}

/// Address range for ranges property
#[derive(Debug, Clone, Copy)]
pub struct AddressRange {
    pub child_bus_addr: u64,
    pub parent_bus_addr: u64,
    pub size: u64,
}

/// Property value
#[derive(Debug, Clone)]
pub enum PropertyValue {
    Empty,
    U32(u32),
    U64(u64),
    String(String),
    StringList(Vec<String>),
    U32List(Vec<u32>),
    U64List(Vec<u64>),
    Bytes(Vec<u8>),
    Reg(Vec<RegBlock>),
    Ranges(Vec<AddressRange>),
}

/// Property (参考 Linux property)
#[derive(Debug, Clone)]
pub struct Property {
    pub name: String,
    pub value: Vec<u8>,
}

impl Property {
    /// Create a new property
    pub fn new(name: String, value: Vec<u8>) -> Self {
        Self { name, value }
    }

    /// Get property as u32 (big-endian)
    pub fn as_u32(&self) -> Option<u32> {
        if self.value.len() >= 4 {
            Some(u32::from_be_bytes([
                self.value[0],
                self.value[1],
                self.value[2],
                self.value[3],
            ]))
        } else {
            None
        }
    }

    /// Get property as u64 (big-endian)
    pub fn as_u64(&self) -> Option<u64> {
        if self.value.len() >= 8 {
            Some(u64::from_be_bytes([
                self.value[0],
                self.value[1],
                self.value[2],
                self.value[3],
                self.value[4],
                self.value[5],
                self.value[6],
                self.value[7],
            ]))
        } else {
            None
        }
    }

    /// Get property as string
    pub fn as_string(&self) -> Option<String> {
        if self.value.is_empty() {
            return None;
        }
        // Remove trailing null byte if present
        let len = if self.value.last() == Some(&0) {
            self.value.len() - 1
        } else {
            self.value.len()
        };
        core::str::from_utf8(&self.value[..len]).map(String::from).ok()
    }

    /// Get property as string list
    pub fn as_string_list(&self) -> Vec<String> {
        let mut result = Vec::new();
        let mut start = 0;

        for (i, &byte) in self.value.iter().enumerate() {
            if byte == 0 && i > start {
                if let Ok(s) = core::str::from_utf8(&self.value[start..i]) {
                    result.push(String::from(s));
                }
                start = i + 1;
            }
        }

        // Handle last string without trailing null
        if start < self.value.len() {
            if let Ok(s) = core::str::from_utf8(&self.value[start..]) {
                result.push(String::from(s));
            }
        }

        result
    }

    /// Get property as u32 list
    pub fn as_u32_list(&self) -> Vec<u32> {
        let mut result = Vec::new();
        let mut i = 0;

        while i + 4 <= self.value.len() {
            result.push(u32::from_be_bytes([
                self.value[i],
                self.value[i + 1],
                self.value[i + 2],
                self.value[i + 3],
            ]));
            i += 4;
        }

        result
    }

    /// Get property as u64 list
    pub fn as_u64_list(&self) -> Vec<u64> {
        let mut result = Vec::new();
        let mut i = 0;

        while i + 8 <= self.value.len() {
            result.push(u64::from_be_bytes([
                self.value[i],
                self.value[i + 1],
                self.value[i + 2],
                self.value[i + 3],
                self.value[i + 4],
                self.value[i + 5],
                self.value[i + 6],
                self.value[i + 7],
            ]));
            i += 8;
        }

        result
    }

    /// Get property as reg blocks
    /// addr_cells and size_cells are from parent node's #address-cells and #size-cells
    pub fn as_reg(&self, addr_cells: u32, size_cells: u32) -> Vec<RegBlock> {
        let mut result = Vec::new();
        let cell_size = 4; // Each cell is 4 bytes
        let entry_size = (addr_cells + size_cells) as usize * cell_size;
        let mut i = 0;

        while i + entry_size <= self.value.len() {
            let address = match addr_cells {
                1 => {
                    let val = u32::from_be_bytes([
                        self.value[i],
                        self.value[i + 1],
                        self.value[i + 2],
                        self.value[i + 3],
                    ]);
                    i += 4;
                    val as u64
                }
                2 => {
                    let val = u64::from_be_bytes([
                        self.value[i],
                        self.value[i + 1],
                        self.value[i + 2],
                        self.value[i + 3],
                        self.value[i + 4],
                        self.value[i + 5],
                        self.value[i + 6],
                        self.value[i + 7],
                    ]);
                    i += 8;
                    val
                }
                _ => 0,
            };

            let size = match size_cells {
                1 => {
                    let val = u32::from_be_bytes([
                        self.value[i],
                        self.value[i + 1],
                        self.value[i + 2],
                        self.value[i + 3],
                    ]);
                    i += 4;
                    val as u64
                }
                2 => {
                    let val = u64::from_be_bytes([
                        self.value[i],
                        self.value[i + 1],
                        self.value[i + 2],
                        self.value[i + 3],
                        self.value[i + 4],
                        self.value[i + 5],
                        self.value[i + 6],
                        self.value[i + 7],
                    ]);
                    i += 8;
                    val
                }
                _ => 0,
            };

            result.push(RegBlock { address, size });
        }

        result
    }

    /// Get property as ranges
    pub fn as_ranges(&self, child_addr_cells: u32, parent_addr_cells: u32, size_cells: u32) -> Vec<AddressRange> {
        let mut result = Vec::new();
        let cell_size = 4; // Each cell is 4 bytes
        let entry_size = (child_addr_cells + parent_addr_cells + size_cells) as usize * cell_size;
        let mut i = 0;

        while i + entry_size <= self.value.len() {
            // Parse child bus address
            let child_bus_addr = match child_addr_cells {
                1 => {
                    let val = u32::from_be_bytes([
                        self.value[i], self.value[i + 1],
                        self.value[i + 2], self.value[i + 3],
                    ]);
                    i += 4;
                    val as u64
                }
                2 => {
                    let val = u64::from_be_bytes([
                        self.value[i], self.value[i + 1], self.value[i + 2], self.value[i + 3],
                        self.value[i + 4], self.value[i + 5], self.value[i + 6], self.value[i + 7],
                    ]);
                    i += 8;
                    val
                }
                _ => 0,
            };

            // Parse parent bus address
            let parent_bus_addr = match parent_addr_cells {
                1 => {
                    let val = u32::from_be_bytes([
                        self.value[i], self.value[i + 1],
                        self.value[i + 2], self.value[i + 3],
                    ]);
                    i += 4;
                    val as u64
                }
                2 => {
                    let val = u64::from_be_bytes([
                        self.value[i], self.value[i + 1], self.value[i + 2], self.value[i + 3],
                        self.value[i + 4], self.value[i + 5], self.value[i + 6], self.value[i + 7],
                    ]);
                    i += 8;
                    val
                }
                _ => 0,
            };

            // Parse size
            let size = match size_cells {
                1 => {
                    let val = u32::from_be_bytes([
                        self.value[i], self.value[i + 1],
                        self.value[i + 2], self.value[i + 3],
                    ]);
                    i += 4;
                    val as u64
                }
                2 => {
                    let val = u64::from_be_bytes([
                        self.value[i], self.value[i + 1], self.value[i + 2], self.value[i + 3],
                        self.value[i + 4], self.value[i + 5], self.value[i + 6], self.value[i + 7],
                    ]);
                    i += 8;
                    val
                }
                _ => 0,
            };

            result.push(AddressRange {
                child_bus_addr,
                parent_bus_addr,
                size,
            });
        }

        result
    }
}

/// Device Node (参考 Linux device_node)
#[derive(Debug, Clone)]
pub struct DeviceNode {
    /// Node name (without unit address)
    pub name: String,
    /// Full path name
    pub full_name: String,
    /// Unit address (part after @)
    pub unit_addr: Option<String>,
    /// Phandle
    pub phandle: Option<u32>,
    /// Properties
    pub properties: BTreeMap<String, Property>,
    /// Child nodes
    pub children: Vec<DeviceNode>,
}

impl DeviceNode {
    /// Create a new device node
    pub fn new(name: String, full_name: String) -> Self {
        // Parse unit address from name (e.g., "cpu@0" -> name="cpu", unit_addr=Some("0"))
        let (base_name, unit_addr) = if let Some(at_pos) = name.find('@') {
            (String::from(&name[..at_pos]), Some(String::from(&name[at_pos + 1..])))
        } else {
            (name, None)
        };

        Self {
            name: base_name,
            full_name,
            unit_addr,
            phandle: None,
            properties: BTreeMap::new(),
            children: Vec::new(),
        }
    }

    /// Get property by name
    pub fn get_property(&self, name: &str) -> Option<&Property> {
        self.properties.get(name)
    }

    /// Get property as string
    pub fn get_string(&self, name: &str) -> Option<String> {
        self.get_property(name)?.as_string()
    }

    /// Get property as u32
    pub fn get_u32(&self, name: &str) -> Option<u32> {
        self.get_property(name)?.as_u32()
    }

    /// Get property as string list
    pub fn get_string_list(&self, name: &str) -> Vec<String> {
        self.get_property(name).map(|p| p.as_string_list()).unwrap_or_default()
    }

    /// Check if node is available (status = "ok" or "okay" or no status property)
    pub fn is_available(&self) -> bool {
        match self.get_string("status") {
            Some(status) => status == "ok" || status == "okay",
            None => true,
        }
    }

    /// Get compatible strings
    pub fn compatible(&self) -> Vec<String> {
        self.get_string_list("compatible")
    }

    /// Get #address-cells
    pub fn address_cells(&self) -> u32 {
        self.get_u32("#address-cells").unwrap_or(2)
    }

    /// Get #size-cells
    pub fn size_cells(&self) -> u32 {
        self.get_u32("#size-cells").unwrap_or(1)
    }

    /// Find child by name
    pub fn find_child(&self, name: &str) -> Option<&DeviceNode> {
        self.children.iter().find(|c| c.name == name)
    }
}

/// Device Tree
#[derive(Debug, Clone)]
pub struct DeviceTree {
    pub root: DeviceNode,
    pub reservations: Vec<super::fdt::MemReservation>,
}

impl DeviceTree {
    pub fn new() -> Self {
        Self {
            root: DeviceNode::new(String::new(), String::from("/")),
            reservations: Vec::new(),
        }
    }

    /// Find node by path
    pub fn find_node(&self, path: &str) -> Option<&DeviceNode> {
        if path == "/" {
            return Some(&self.root);
        }

        let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
        let mut current = &self.root;

        for part in parts {
            current = current.find_child(part)?;
        }

        Some(current)
    }

    /// Find nodes by compatible string
    pub fn find_compatible(&self, compatible: &str) -> Vec<&DeviceNode> {
        let mut result = Vec::new();
        self.find_compatible_recursive(&self.root, compatible, &mut result);
        result
    }

    fn find_compatible_recursive<'a>(&self, node: &'a DeviceNode, compatible: &str, result: &mut Vec<&'a DeviceNode>) {
        if node.compatible().iter().any(|c| c == compatible) {
            result.push(node);
        }
        for child in &node.children {
            self.find_compatible_recursive(child, compatible, result);
        }
    }
}

impl Default for DeviceTree {
    fn default() -> Self {
        Self::new()
    }
}