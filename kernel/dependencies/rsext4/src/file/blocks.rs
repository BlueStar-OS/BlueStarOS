use super::*;

/// Builds block mappings for a file inode from absolute data block numbers.
pub fn build_file_block_mapping<B: BlockDevice>(
    fs: &mut Ext4FileSystem,
    inode: &mut Ext4Inode,
    data_blocks: &[AbsoluteBN],
    block_dev: &mut Jbd2Dev<B>,
) {
    if data_blocks.is_empty() {
        inode.i_blocks_lo = 0;
        inode.l_i_blocks_high = 0;
        inode.i_block = [0; 15];
        return;
    }

    if fs.superblock.has_extents() {
        // 使用 extent 映射数据块，优先合并连续块
        inode.i_flags |= Ext4Inode::EXT4_EXTENTS_FL;
        inode.i_block = [0; 15];

        //初始头构建
        if !inode.have_extend_header_and_use_extend() {
            inode.i_flags |= Ext4Inode::EXT4_EXTENTS_FL;
            inode.write_extend_header();
        }

        let mut exts_vec: Vec<Ext4Extent> = Vec::new();

        let mut run_start_lbn: u32 = 0;
        let mut run_start_pblk = data_blocks[0].raw();
        let mut run_len: u32 = 1;

        for (idx, &pblk) in data_blocks.iter().enumerate().skip(1) {
            let lbn = idx as u32;
            let prev_lbn = lbn - 1;
            let prev_pblk = data_blocks[prev_lbn as usize].raw();
            let pblk = pblk.raw();

            let is_contiguous = pblk == prev_pblk.saturating_add(1);

            if is_contiguous {
                run_len = run_len.saturating_add(1);
            } else {
                // 结束当前 run，生成一个 extent
                let ext = Ext4Extent::new(run_start_lbn, run_start_pblk, run_len as u16);
                exts_vec.push(ext);

                run_start_lbn = lbn;
                run_start_pblk = pblk;
                run_len = 1;
            }
        }

        let ext = Ext4Extent::new(run_start_lbn, run_start_pblk, run_len as u16);
        exts_vec.push(ext);

        // 构造一个叶子根节点，并通过 ExtentTree 将其写入 inode.i_block
        let mut tree = ExtentTree::new(inode);
        for extend in exts_vec {
            tree.insert_extent(fs, extend, block_dev)
                .expect("Extend insert Failed!");
        }
    } else {
        error!("not support tranditional block pointer");
    }
}
