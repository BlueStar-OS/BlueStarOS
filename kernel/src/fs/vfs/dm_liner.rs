//! dm-linear 线性映射：用一对 (起始扇区, 扇区数) 描述块设备上的一段连续区域。
//!
//! ## 设计定位
//!
//! 无论是原始整盘（Raw）、MBR/GPT 分区，还是 LVM 逻辑卷中的一段 extent，
//! 它们的运行时语义都退化为同一种表述：
//!
//! > "从这个 backing device 的 `start_lba` 开始，取 `sectors` 个扇区。"
//!
//! `DmlinerEntry` 就是这个最小公共抽象——它不区分分区格式、不关心
//! 元数据来源，只提供运行时 LBA 翻译所需的两个数值。

/// dm-linear 的一条映射条目。
///
/// 字段均为 `pub` 以允许调用方直接构造；只读语义下修改无意义。
#[derive(Clone, Copy, Debug)]
pub struct DmlinerEntry {
    /// 在 backing device 上的起始扇区号（LBA）
    pub start_lba: u64,
    /// 从 `start_lba` 开始的连续扇区数
    pub sectors: u64,
}

impl DmlinerEntry {
    pub const fn new(start_lba: u64, sectors: u64) -> Self {
        Self { start_lba, sectors }
    }
}
