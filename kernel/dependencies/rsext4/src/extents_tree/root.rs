use super::*;

/// 绑定到单个 inode 的 extent 树视图（不持有 BlockDev，按需传入）
pub struct ExtentTree<'a> {
    pub inode: &'a mut Ext4Inode,
}

impl<'a> ExtentTree<'a> {
    /// 构造：从给定 inode 开始操作其 extent 树
    pub fn new(inode: &'a mut Ext4Inode) -> Self {
        Self { inode }
    }

    pub(super) fn add_inode_sectors_for_block(&mut self) {
        let add_sectors = (BLOCK_SIZE / 512) as u64;
        let cur = ((self.inode.l_i_blocks_high as u64) << 32) | (self.inode.i_blocks_lo as u64);
        let newv = cur.saturating_add(add_sectors);
        self.inode.i_blocks_lo = (newv & 0xFFFF_FFFF) as u32;
        self.inode.l_i_blocks_high = ((newv >> 32) & 0xFFFF) as u16;
    }

    pub(super) fn sub_inode_sectors_for_block(&mut self) {
        let sub_sectors = (BLOCK_SIZE / 512) as u64;
        let cur = ((self.inode.l_i_blocks_high as u64) << 32) | (self.inode.i_blocks_lo as u64);
        let newv = cur.saturating_sub(sub_sectors);
        self.inode.i_blocks_lo = (newv & 0xFFFF_FFFF) as u32;
        self.inode.l_i_blocks_high = ((newv >> 32) & 0xFFFF) as u16;
    }

    /// 从 inode.i_block 解析根节点
    pub fn load_root_from_inode(&self) -> Option<ExtentNode> {
        // inode.i_block 是 15 * u32 = 60 字节，正好容纳一个 extent 节点
        let iblocks = &self.inode.i_block; //不同端序解析为错误端序
        let mut bytes: [u8; 60] = [0; 60];
        for idx in 0..15 {
            //正确处理字节序
            let trans_b1 = iblocks[idx].to_le_bytes();
            bytes[idx * 4] = trans_b1[0];
            bytes[idx * 4 + 1] = trans_b1[1];
            bytes[idx * 4 + 2] = trans_b1[2];
            bytes[idx * 4 + 3] = trans_b1[3];
        }
        Self::parse_node_from_bytes(&bytes)
    }

    /// 将根节点写回 inode.i_block
    pub fn store_root_to_inode(&mut self, node: &ExtentNode) {
        let hdr_size = Ext4ExtentHeader::disk_size();

        match node {
            ExtentNode::Leaf { header, entries } => {
                // 仅支持 depth=0：header + 若干 Ext4Extent 写入到 i_block（60 字节）
                let mut buf = [0u8; 60];

                // 写 header
                header.to_disk_bytes(&mut buf[0..hdr_size]);

                // 写 extents
                let et_size = Ext4Extent::disk_size();
                for (i, e) in entries.iter().enumerate() {
                    let off = hdr_size + i * et_size;
                    if off + et_size > buf.len() {
                        break;
                    }
                    e.to_disk_bytes(&mut buf[off..off + et_size]);
                }

                // 将 60 字节解释为 15 个 u32 写回 i_block
                for i in 0..15 {
                    let off = i * 4;
                    let v =
                        u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]]);
                    self.inode.i_block[i] = v;
                }
            }
            ExtentNode::Index { header, entries } => {
                // depth>0：header + 若干 Ext4ExtentIdx 写入到 inode.i_block
                let mut buf = [0u8; 60];

                header.to_disk_bytes(&mut buf[0..hdr_size]);

                let idx_size = Ext4ExtentIdx::disk_size();
                for (i, idx) in entries.iter().enumerate() {
                    let off = hdr_size + i * idx_size;
                    if off + idx_size > buf.len() {
                        break;
                    }
                    idx.to_disk_bytes(&mut buf[off..off + idx_size]);
                }

                for i in 0..15 {
                    let off = i * 4;
                    let v =
                        u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]]);
                    self.inode.i_block[i] = v;
                }
            }
        }
    }

    /// Writes an extent node to an absolute physical block.
    pub(super) fn write_node_to_block<B: BlockDevice>(
        dev: &mut Jbd2Dev<B>,
        block_id: AbsoluteBN,
        node: &ExtentNode,
        eh_max: u16,
    ) -> Ext4Result<()> {
        let hdr_size = Ext4ExtentHeader::disk_size();
        // 读取块
        dev.read_block(block_id)?;
        let buf = dev.buffer_mut();

        match node {
            ExtentNode::Leaf { header, entries } => {
                let et_size = Ext4Extent::disk_size();
                // 确保 header 中的 max 正确（因为内存中的 node 可能来自 root，max 很小）
                let mut disk_header = *header;
                disk_header.eh_max = eh_max;
                // 写 header
                disk_header.to_disk_bytes(&mut buf[0..hdr_size]);
                // 写 extents
                for (i, e) in entries.iter().enumerate() {
                    let off = hdr_size + i * et_size;
                    if off + et_size > buf.len() {
                        break;
                    }
                    e.to_disk_bytes(&mut buf[off..off + et_size]);
                }
            }
            ExtentNode::Index { header, entries } => {
                let idx_size = Ext4ExtentIdx::disk_size();
                let mut disk_header = *header;
                disk_header.eh_max = eh_max;

                // 写 header
                disk_header.to_disk_bytes(&mut buf[0..hdr_size]);
                // 写索引
                for (i, idx) in entries.iter().enumerate() {
                    let off = hdr_size + i * idx_size;
                    if off + idx_size > buf.len() {
                        break;
                    }
                    idx.to_disk_bytes(&mut buf[off..off + idx_size]);
                }
            }
        }
        // 标记脏并写回
        dev.write_block(block_id, true)?;
        Ok(())
    }
}
