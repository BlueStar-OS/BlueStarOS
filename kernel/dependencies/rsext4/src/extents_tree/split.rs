use super::*;

/// 用于在递归插入时向上冒泡分裂信息
pub(super) struct SplitInfo {
    ///分裂出去的右节点的起始逻辑块号 (Key)
    pub(super) start_block: u32,
    ///分裂出去的右节点的物理块号 (Value)
    pub(super) phy_block: AbsoluteBN,
}

impl<'a> ExtentTree<'a> {
    /// 计算标准数据块能容纳的条目数
    pub(super) fn calc_block_eh_max() -> u16 {
        let hdr_size = Ext4ExtentHeader::disk_size();
        let entry_size = Ext4Extent::disk_size(); // Index 和 Extent 大小一样，都是 12
        (BLOCK_SIZE.saturating_sub(hdr_size) / entry_size) as u16
    }

    /// 辅助：获取节点的起始逻辑块号
    pub(super) fn get_node_start_block(node: &ExtentNode) -> u32 {
        match node {
            ExtentNode::Leaf { entries, .. } => {
                if entries.is_empty() {
                    0
                } else {
                    entries[0].ee_block
                }
            }
            ExtentNode::Index { entries, .. } => {
                if entries.is_empty() {
                    0
                } else {
                    entries[0].ei_block
                }
            }
        }
    }
}
