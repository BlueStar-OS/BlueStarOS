use super::*;

/// 内存中的 extent 树节点表示
#[derive(Clone)]
pub enum ExtentNode {
    /// 叶子节点：header.eh_depth == 0，后面跟 Ext4Extent
    Leaf {
        header: Ext4ExtentHeader,
        entries: Vec<Ext4Extent>,
    },
    /// 内部节点：header.eh_depth > 0，后面跟 Ext4ExtentIdx
    Index {
        header: Ext4ExtentHeader,
        entries: Vec<Ext4ExtentIdx>,
    },
}

impl ExtentNode {
    pub fn header(&self) -> &Ext4ExtentHeader {
        match self {
            ExtentNode::Leaf { header, .. } => header,
            ExtentNode::Index { header, .. } => header,
        }
    }

    pub fn header_mut(&mut self) -> &mut Ext4ExtentHeader {
        match self {
            ExtentNode::Leaf { header, .. } => header,
            ExtentNode::Index { header, .. } => header,
        }
    }

    pub fn is_leaf(&self) -> bool {
        matches!(self, ExtentNode::Leaf { .. })
    }
}
