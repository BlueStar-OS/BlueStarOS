//! DTB Parser
//! Reference: Linux 5.4.29 drivers/of/fdt.c

use alloc::string::String;
use alloc::vec::Vec;
use alloc::collections::BTreeMap;
use alloc::format;

use super::fdt::{FdtHeader, FdtError, FDT_BEGIN_NODE, FDT_END_NODE, FDT_PROP, FDT_NOP, FDT_END, MemReservation};
use super::node::{DeviceNode, DeviceTree, Property};

/// DTB Parser
pub struct DtbParser<'a> {
    raw: &'a [u8],
    header: FdtHeader,
}

impl<'a> DtbParser<'a> {
    /// Create a new DTB parser
    /// Reference: Linux early_init_dt_verify
    pub fn new(raw: &'a [u8]) -> Result<Self, FdtError> {
        let header = FdtHeader::from_bytes(raw).ok_or(FdtError::Truncated)?;
        header.validate()?;

        // Check if totalsize is reasonable
        if header.totalsize as usize > raw.len() {
            return Err(FdtError::Truncated);
        }

        Ok(Self { raw, header })
    }

    /// Get header
    pub fn header(&self) -> &FdtHeader {
        &self.header
    }

    /// Get magic number
    pub fn magic(&self) -> u32 {
        self.header.magic
    }

    /// Get version
    pub fn version(&self) -> u32 {
        self.header.version
    }

    /// Get total size
    pub fn totalsize(&self) -> u32 {
        self.header.totalsize
    }

    /// Read u32 from raw bytes (big-endian)
    fn read_u32(&self, offset: usize) -> u32 {
        u32::from_be_bytes([
            self.raw[offset],
            self.raw[offset + 1],
            self.raw[offset + 2],
            self.raw[offset + 3],
        ])
    }

    /// Read u64 from raw bytes (big-endian)
    fn read_u64(&self, offset: usize) -> u64 {
        u64::from_be_bytes([
            self.raw[offset],
            self.raw[offset + 1],
            self.raw[offset + 2],
            self.raw[offset + 3],
            self.raw[offset + 4],
            self.raw[offset + 5],
            self.raw[offset + 6],
            self.raw[offset + 7],
        ])
    }

    /// Get string from strings block
    /// Reference: Linux fdt_get_string
    fn get_string(&self, offset: u32) -> &str {
        let offset = offset as usize;
        let strings_start = self.header.off_dt_strings as usize;

        if strings_start + offset >= self.raw.len() {
            return "";
        }

        let start = strings_start + offset;
        let end = self.raw[start..].iter().position(|&b| b == 0)
            .map(|p| start + p)
            .unwrap_or(self.raw.len());

        core::str::from_utf8(&self.raw[start..end]).unwrap_or("")
    }

    /// Get null-terminated string from structure block
    fn get_node_name(&self, offset: usize) -> &str {
        if offset >= self.raw.len() {
            return "";
        }

        let end = self.raw[offset..].iter().position(|&b| b == 0)
            .map(|p| offset + p)
            .unwrap_or(self.raw.len());

        core::str::from_utf8(&self.raw[offset..end]).unwrap_or("")
    }

    /// Align offset to 4 bytes
    fn align4(&self, offset: usize) -> usize {
        (offset + 3) & !3
    }

    /// Get memory reservations
    /// Reference: Linux fdt_get_mem_rsv
    pub fn mem_reservations(&self) -> impl Iterator<Item = MemReservation> + '_ {
        let mut offset = self.header.off_mem_rsvmap as usize;
        let entry_size = 16; // 2 x u64

        core::iter::from_fn(move || {
            if offset + entry_size > self.raw.len() {
                return None;
            }

            let address = self.read_u64(offset);
            let size = self.read_u64(offset + 8);
            offset += entry_size;

            // End of reservations
            if address == 0 && size == 0 {
                return None;
            }

            Some(MemReservation { address, size })
        })
    }

    /// Parse the device tree
    /// Reference: Linux unflatten_dt_nodes
    pub fn parse(&self) -> Result<DeviceTree, FdtError> {
        let mut tree = DeviceTree::new();

        // Parse memory reservations
        tree.reservations = self.mem_reservations().collect();

        // Parse structure block
        let mut offset = self.header.off_dt_struct as usize;
        let struct_end = offset + self.header.size_dt_struct as usize;

        // Stack for tracking parent nodes
        let mut node_stack: Vec<DeviceNode> = Vec::new();
        let mut path_stack: Vec<String> = Vec::new();

        while offset < struct_end {
            let tag = self.read_u32(offset);
            offset += 4;

            match tag {
                FDT_BEGIN_NODE => {
                    // Read node name
                    let name = self.get_node_name(offset);
                    offset = self.align4(offset + name.len() + 1);

                    // Build full path
                    let full_name = if path_stack.is_empty() {
                        String::from("/")
                    } else {
                        let path = path_stack.join("/");
                        if path.is_empty() {
                            String::from("/")
                        } else {
                            format!("/{}", path)
                        }
                    };

                    // Create new node
                    let node = DeviceNode::new(
                        String::from(name),
                        full_name,
                    );

                    node_stack.push(node);
                    path_stack.push(String::from(name));
                }

                FDT_END_NODE => {
                    if let Some(node) = node_stack.pop() {
                        path_stack.pop();

                        if let Some(parent) = node_stack.last_mut() {
                            parent.children.push(node);
                        } else {
                            // This is the root node
                            tree.root = node;
                        }
                    }
                }

                FDT_PROP => {
                    // Read property length and name offset
                    let len = self.read_u32(offset) as usize;
                    let nameoff = self.read_u32(offset + 4);
                    offset += 8;

                    // Get property name
                    let name = self.get_string(nameoff);

                    // Get property value
                    let value = if len > 0 && offset + len <= self.raw.len() {
                        self.raw[offset..offset + len].to_vec()
                    } else {
                        Vec::new()
                    };

                    // Align to 4 bytes
                    offset = self.align4(offset + len);

                    // Add property to current node
                    if let Some(node) = node_stack.last_mut() {
                        node.properties.insert(
                            String::from(name),
                            Property::new(String::from(name), value),
                        );

                        // Handle phandle
                        if name == "phandle" || name == "linux,phandle" {
                            if let Some(phandle) = node.properties.get(name).and_then(|p| p.as_u32()) {
                                node.phandle = Some(phandle);
                            }
                        }
                    }
                }

                FDT_NOP => {
                    // Skip NOP
                }

                FDT_END => {
                    break;
                }

                _ => {
                    return Err(FdtError::BadStructure);
                }
            }
        }

        Ok(tree)
    }
}