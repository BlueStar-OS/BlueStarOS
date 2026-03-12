//! FDT (Flattened Device Tree) structures and constants
//! Reference: Linux 5.4.29 scripts/dtc/libfdt/fdt.h

/// FDT Magic Number
pub const FDT_MAGIC: u32 = 0xd00dfeed;

/// FDT Tags
pub const FDT_BEGIN_NODE: u32 = 0x1;  // Start node: full name
pub const FDT_END_NODE: u32 = 0x2;    // End node
pub const FDT_PROP: u32 = 0x3;         // Property: name off, size, content
pub const FDT_NOP: u32 = 0x4;          // nop
pub const FDT_END: u32 = 0x9;          // End

/// FDT supported versions
pub const FDT_FIRST_SUPPORTED_VERSION: u32 = 0x02;
pub const FDT_LAST_SUPPORTED_VERSION: u32 = 0x11;

/// DTB Header (40 bytes)
/// Reference: Linux fdt_header
#[repr(C)]
pub struct FdtHeader {
    pub magic: u32,              // magic word FDT_MAGIC
    pub totalsize: u32,          // total size of DT block
    pub off_dt_struct: u32,      // offset to structure
    pub off_dt_strings: u32,     // offset to strings
    pub off_mem_rsvmap: u32,     // offset to memory reserve map
    pub version: u32,            // format version
    pub last_comp_version: u32,  // last compatible version
    // version 2 fields below
    pub boot_cpuid_phys: u32,    // Which physical CPU id we're booting on
    // version 3 fields below
    pub size_dt_strings: u32,    // size of the strings block
    // version 17 fields below
    pub size_dt_struct: u32,     // size of the structure block
}

/// Memory Reservation Entry
/// Reference: Linux fdt_reserve_entry
#[repr(C)]
pub struct FdtReserveEntry {
    pub address: u64,
    pub size: u64,
}

/// FDT Node Header
/// Reference: Linux fdt_node_header
#[repr(C)]
pub struct FdtNodeHeader {
    pub tag: u32,
    pub name: [u8; 0],  // Flexible array member
}

/// FDT Property
/// Reference: Linux fdt_property
#[repr(C)]
pub struct FdtProperty {
    pub tag: u32,
    pub len: u32,
    pub nameoff: u32,
    pub data: [u8; 0],  // Flexible array member
}

/// FDT Error codes
/// Reference: Linux libfdt.h
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FdtError {
    NotFound = 1,       // The requested node or property does not exist
    Exists = 2,         // Attempted to create a node or property which already exists
    NoSpace = 3,        // Operation needed to expand the device tree, but no space
    BadOffset = 4,      // Function was passed a structure block offset which is out-of-bounds
    BadPath = 5,        // Function was passed a badly formatted path
    BadPhandle = 6,     // Function was passed an invalid phandle
    BadState = 7,       // Function was passed an incomplete device tree
    Truncated = 8,      // FDT or a sub-block is improperly terminated
    BadMagic = 9,       // Given "device tree" appears not to be a device tree at all
    BadVersion = 10,    // Given device tree has a version which can't be handled
    BadStructure = 11,  // Given device tree has a corrupt structure block
    BadLayout = 12,     // The given device tree has it's sub-blocks in an unexpected order
    Internal = 13,      // libfdt has failed an internal assertion
    BadNCells = 14,     // Device tree has a #address-cells, #size-cells or similar property with a bad format
    BadValue = 15,      // Device tree has a property with an unexpected value
    BadOverlay = 16,    // The device tree overlay cannot be applied
    NoPhandles = 17,    // The device tree doesn't have any phandle available
    BadFlags = 18,      // The function was passed a flags field with invalid flags
}

/// Memory Reservation
#[derive(Debug, Clone, Copy)]
pub struct MemReservation {
    pub address: u64,
    pub size: u64,
}

impl FdtHeader {
    /// Read big-endian u32 from raw bytes
    #[inline]
    fn read_be_u32(bytes: &[u8], offset: usize) -> u32 {
        u32::from_be_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ])
    }

    /// Parse header from raw bytes
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 40 {
            return None;
        }

        Some(Self {
            magic: Self::read_be_u32(bytes, 0),
            totalsize: Self::read_be_u32(bytes, 4),
            off_dt_struct: Self::read_be_u32(bytes, 8),
            off_dt_strings: Self::read_be_u32(bytes, 12),
            off_mem_rsvmap: Self::read_be_u32(bytes, 16),
            version: Self::read_be_u32(bytes, 20),
            last_comp_version: Self::read_be_u32(bytes, 24),
            boot_cpuid_phys: Self::read_be_u32(bytes, 28),
            size_dt_strings: Self::read_be_u32(bytes, 32),
            size_dt_struct: Self::read_be_u32(bytes, 36),
        })
    }

    /// Validate the header
    pub fn validate(&self) -> Result<(), FdtError> {
        if self.magic != FDT_MAGIC {
            return Err(FdtError::BadMagic);
        }

        if self.version < FDT_FIRST_SUPPORTED_VERSION
            || self.version > FDT_LAST_SUPPORTED_VERSION {
            return Err(FdtError::BadVersion);
        }

        Ok(())
    }
}