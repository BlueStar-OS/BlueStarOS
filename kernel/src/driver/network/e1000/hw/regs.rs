//! e1000 寄存器偏移量与位定义。
//!
//! 参考 Linux:
//! - drivers/net/ethernet/intel/e1000/e1000_hw.h
//! - drivers/net/ethernet/intel/e1000/e1000_main.c

/// e1000 支持的 PCI device id 列表。
/// 参考 Linux: drivers/net/ethernet/intel/e1000/e1000_main.c:24-64
pub(crate) const E1000_DEVICE_IDS: &[u16] = &[
    0x1000, 0x1001, 0x1004, 0x1008, 0x1009, 0x100C, 0x100D, 0x100E, 0x100F, 0x1010, 0x1011, 0x1012,
    0x1013, 0x1014, 0x1015, 0x1016, 0x1017, 0x1018, 0x1019, 0x101A, 0x101D, 0x101E, 0x1026, 0x1027,
    0x1028, 0x1075, 0x1076, 0x1077, 0x1078, 0x1079, 0x107A, 0x107B, 0x107C, 0x108A, 0x1099, 0x10B5,
    0x2E6E,
];

pub(crate) const E1000_CTRL: usize = 0x00000;
pub(crate) const E1000_STATUS: usize = 0x00008;
pub(crate) const E1000_EECD: usize = 0x00010;
pub(crate) const E1000_EERD: usize = 0x00014;
pub(crate) const E1000_ICR: usize = 0x000C0;
pub(crate) const E1000_IMS: usize = 0x000D0;
pub(crate) const E1000_IMC: usize = 0x000D8;
pub(crate) const E1000_RCTL: usize = 0x00100;
pub(crate) const E1000_TCTL: usize = 0x00400;
pub(crate) const E1000_RA: usize = 0x05400;

pub(crate) const E1000_ICR_TXDW: u32 = 0x00000001;
pub(crate) const E1000_ICR_LSC: u32 = 0x00000004;
pub(crate) const E1000_ICR_RXDMT0: u32 = 0x00000010;
pub(crate) const E1000_ICR_RXO: u32 = 0x00000040;
pub(crate) const E1000_ICR_RXT0: u32 = 0x00000080;

pub(crate) const E1000_IMS_ENABLE_MASK: u32 =
    E1000_ICR_RXT0 | E1000_ICR_RXO | E1000_ICR_RXDMT0 | E1000_ICR_LSC;

pub(crate) const E1000_CTRL_SLU: u32 = 0x00000040;
pub(crate) const E1000_CTRL_RST: u32 = 0x04000000;

pub(crate) const E1000_STATUS_FD: u32 = 0x00000001;
pub(crate) const E1000_STATUS_LU: u32 = 0x00000002;

pub(crate) const E1000_TCTL_PSP: u32 = 0x00000008;
pub(crate) const E1000_RAH_AV: u32 = 0x80000000;

pub(crate) const E1000_EERD_START: u32 = 0x00000001;
pub(crate) const E1000_EERD_DONE: u32 = 0x00000010;
pub(crate) const E1000_EERD_ADDR_SHIFT: u32 = 8;
pub(crate) const E1000_EERD_DATA_SHIFT: u32 = 16;

pub(crate) const E1000_NODE_ADDRESS_SIZE: usize = 6;

/// 从硬件中读出的 MAC 原始寄存器视图。
pub(crate) struct ReadMacRaw {
    pub(crate) ral: u32,
    pub(crate) rah: u32,
}
