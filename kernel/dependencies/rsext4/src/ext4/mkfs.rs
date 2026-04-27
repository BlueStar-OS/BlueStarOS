use super::*;

/// 文件系统布局信息（仅用于 mkfs 阶段的计算）
pub struct FsLayoutInfo {
    /// 逻辑块大小（字节）
    block_size: u32,
    /// 每组块数
    blocks_per_group: u32,
    /// 每组 inode 数
    inodes_per_group: u32,
    /// inode 大小（字节）
    inode_size: u16,
    /// 块组数
    groups: u32,
    /// 块组描述符大小（字节）
    desc_size: u16,
    /// 每块能容纳的组描述符个数
    descs_per_block: u32,
    /// 主 GDT 实际占用的块数
    gdt_blocks: u32,
    /// 每组 inode 表占用的块数
    inode_table_blocks: u32,
    /// 第一个数据块号（对应 s_first_data_block）
    first_data_block: u32,
    /// 预留的 GDT 块数（应等于 RESERVED_GDT_BLOCKS）
    reserved_gdt_blocks: u32,
    /// 组0的块位图块号
    group0_block_bitmap: u32,
    /// 组0的 inode 位图块号
    group0_inode_bitmap: u32,
    /// 组0的 inode 表起始块号
    group0_inode_table: u32,
    /// 组0中元数据占用的块数
    group0_metadata_blocks: u32,
    /// 预留块总数（按比例预留给 root）
    reserved_blocks: u64,
}

/// block_group 布局信息，仅在 mkfs 阶段使用
pub struct BlcokGroupLayout {
    /// 块组起始块号（全局块号）
    pub group_start_block: u64,
    /// 块组内块位图所在的块号（全局块号）
    pub group_blcok_bitmap_startblocks: u64,
    /// 块组内 inode 位图所在的块号（全局块号）
    pub group_inode_bitmap_startblocks: u64,
    /// 块组内 inode 表起始块号（全局块号）
    pub group_inode_table_startblocks: u64,
    /// 该块组中元数据占用的块数（引导/备份 super+GDT+位图+inode 表）
    pub metadata_blocks_in_group: u32,
}

pub fn compute_fs_layout(inode_size: u16, total_blocks: u64) -> FsLayoutInfo {
    let block_size: u32 = 1024u32 << LOG_BLOCK_SIZE;

    // 每组块数：8 * block_size（标准 ext4 默认）
    let blocks_per_group: u32 = 8 * block_size;

    // 每组 inode 数：blocks_per_group / 4（简化策略）
    let inodes_per_group: u32 = blocks_per_group / 4;

    // 块组数：向上取整
    let groups: u32 = total_blocks.div_ceil(blocks_per_group as u64) as u32;

    // 确定块组描述符大小，默认使用64位描述符大小，除非明确指定使用32位
    let desc_size: u16 =
        if DEFAULT_FEATURE_INCOMPAT & Ext4Superblock::EXT4_FEATURE_INCOMPAT_64BIT != 0 {
            GROUP_DESC_SIZE
        } else {
            GROUP_DESC_SIZE_OLD
        };

    // 每块能容纳的组描述符个数
    let descs_per_block: u32 = if desc_size == 0 {
        0
    } else {
        block_size / desc_size as u32
    };

    // GDT 实际占用的块数
    let gdt_blocks: u32 = if descs_per_block == 0 {
        0
    } else {
        groups.div_ceil(descs_per_block)
    };

    // 每组 inode 表占用的块数
    let inode_table_blocks: u32 = if block_size == 0 {
        0
    } else {
        (inodes_per_group * inode_size as u32).div_ceil(block_size)
    };

    // 第一个数据块：块大小 > 1024 时为 0，否则为 1（参考 lwext4 create_fs_aux_info）
    let first_data_block: u32 = if block_size > 1024 { 0 } else { 1 };

    // 预留的 GDT 块数（与 ext4 标准一致）
    let reserved_gdt_blocks: u32 = RESERVED_GDT_BLOCKS;

    // 组0布局：
    // - 对于 4K：Primary superblock at 0, GDT at 1, Reserved GDT blocks at 2..(2+reserved_gdt_blocks-1)
    // - 我们在预留 GDT 区域之后顺序放置 block_bitmap、inode_bitmap、inode_table
    let group0_start: u32 = first_data_block;
    let reserved_gdt_start: u32 = group0_start + 2; // 块0=引导/超级块，块1=GDT，块2.. 预留GDT
    let group0_block_bitmap: u32 = reserved_gdt_start + reserved_gdt_blocks; // 2 + reserved
    let group0_inode_bitmap: u32 = group0_block_bitmap + 1;
    let group0_inode_table: u32 = group0_inode_bitmap + 1;
    let group0_metadata_blocks: u32 = (group0_inode_table + inode_table_blocks) - group0_start;

    // 预留块总数：约 5%（与 ext4 默认类似）
    let reserved_blocks: u64 = total_blocks / 20; // 5%

    FsLayoutInfo {
        block_size,
        blocks_per_group,
        inodes_per_group,
        inode_size,
        groups,
        desc_size,
        descs_per_block,
        gdt_blocks,
        inode_table_blocks,
        first_data_block,
        reserved_gdt_blocks,
        group0_block_bitmap,
        group0_inode_bitmap,
        group0_inode_table,
        group0_metadata_blocks,
        reserved_blocks,
    }
}

pub fn mkfs<B: BlockDevice>(block_dev: &mut Jbd2Dev<B>) -> Ext4Result<()> {
    debug!("Start initializing Ext4 filesystem...");
    // mkfs 阶段先强制关闭日志，避免还未初始化 journal superblock 时触发 JBD2 逻辑
    block_dev.set_journal_use(false);
    let old_jouranl_use = block_dev.is_use_journal();

    // 1. 计算布局参数
    let total_blocks = block_dev.total_blocks();
    let layout = compute_fs_layout(DEFAULT_INODE_SIZE, total_blocks);
    let total_groups = layout.groups;

    debug!("  Total blocks: {total_blocks}");
    debug!("  Block size: {} bytes", layout.block_size);
    debug!("  Block group count: {total_groups}");
    debug!("  Blocks per group: {}", layout.blocks_per_group);
    debug!("  Inodes per group: {}", layout.inodes_per_group);

    //构建并根据fearure写入到所有group超级块
    let superblock = build_superblock(total_blocks, &layout);
    write_superblock(block_dev, &superblock)?;
    debug!("Superblock written");

    //写冗余备份 自动判断是否写
    write_superblock_redundant_backup(block_dev, &superblock, total_groups, &layout)?;

    //注意顺序
    let mut descs: VecDeque<Ext4GroupDesc> = VecDeque::new();
    //为superblock写入gdt（全部标记为UNINIT）
    for group_id in 0..total_groups {
        let mut desc = build_uninit_group_desc(&superblock, group_id, &layout);
        write_group_desc(block_dev, group_id, &mut desc)?;
        descs.push_back(desc);
    }
    //为其它块组选择性的写入冗余备份desc
    write_gdt_redundant_backup(block_dev, &descs, &superblock, total_groups, &layout)?;
    debug!("{total_groups} block group descriptors written");

    //实际初始化块组0（用于根目录）
    initialize_group_0(block_dev, &layout)?;
    debug!("Block group 0 initialized (for root directory)");

    // 初始化其它块组的位图（全部视为空闲）
    initialize_other_groups_bitmaps(block_dev, &layout, &superblock)?;

    let mut initialized_descs: VecDeque<Ext4GroupDesc> = VecDeque::new();
    for group_id in 0..total_groups {
        let mut desc = build_uninit_group_desc(&superblock, group_id, &layout);
        if group_id == 0 {
            desc.bg_flags = Ext4GroupDesc::EXT4_BG_INODE_ZEROED;
        }
        write_group_desc(block_dev, group_id, &mut desc)?;
        initialized_descs.push_back(desc);
    }
    write_gdt_redundant_backup(
        block_dev,
        &initialized_descs,
        &superblock,
        total_groups,
        &layout,
    )?;

    //通过一次挂载/卸载流程，让根目录在 mkfs 阶段就被真正创建并写回磁盘
    // 注意：此时日志仍然关闭，等真正挂载时再开启 JBD2
    {
        let mut fs = Ext4FileSystem::mount(block_dev).expect("Mount Failed!");
        fs.umount(block_dev)?;
    }

    //  验证：读回超级块检查魔数
    let verify_sb = read_superblock(block_dev)?;

    // mkfs 结束前恢复日志开关（为后续真实挂载做准备）
    block_dev.set_journal_use(old_jouranl_use);

    if verify_sb.s_magic == EXT4_SUPER_MAGIC {
        debug!(
            "Format completed, superblock magic verified: {:#x}",
            verify_sb.s_magic
        );
        Ok(())
    } else {
        debug!("Superblock magic verification failed");
        Err(Ext4Error::corrupted())
    }
}

/// 构建超级块 不管字节序
fn build_superblock(total_blocks: u64, layout: &FsLayoutInfo) -> Ext4Superblock {
    let mut sb = Ext4Superblock {
        s_magic: EXT4_SUPER_MAGIC,
        s_blocks_count_lo: (total_blocks & 0xFFFFFFFF) as u32,
        s_blocks_count_hi: (total_blocks >> 32) as u32,
        s_log_block_size: LOG_BLOCK_SIZE,
        s_log_cluster_size: LOG_BLOCK_SIZE,
        s_blocks_per_group: layout.blocks_per_group,
        s_inodes_per_group: layout.inodes_per_group,
        s_clusters_per_group: layout.blocks_per_group,
        s_inodes_count: layout.groups * layout.inodes_per_group,
        s_inode_size: layout.inode_size,
        s_first_ino: RESERVED_INODES + 1,
        s_first_data_block: layout.first_data_block,
        s_r_blocks_count_lo: (layout.reserved_blocks & 0xFFFFFFFF) as u32,
        s_r_blocks_count_hi: (layout.reserved_blocks >> 32) as u32,
        ..Default::default()
    };

    //设置hash种子
    //需要生成UUID
    let uuid = generate_uuid();
    sb.s_hash_seed = uuid.0;

    //设置文件系统UUID
    let filesys_uuid = generate_uuid_8();
    sb.s_uuid = filesys_uuid;

    // 空闲计数：总块数 - 组0元数据块数 - 预留块数（其余组初始全空闲）
    let metadata_blocks = layout.group0_metadata_blocks as u64;
    let mut free_blocks = total_blocks
        .saturating_sub(metadata_blocks)
        .saturating_sub(layout.reserved_blocks);
    if free_blocks > total_blocks {
        free_blocks = 0;
    }
    sb.s_free_blocks_count_lo = (free_blocks & 0xFFFFFFFF) as u32;
    sb.s_free_blocks_count_hi = (free_blocks >> 32) as u32;

    sb.s_min_extra_isize = 32;
    sb.s_want_extra_isize = 32;

    // 预留 inode（1-RESERVED_INODES）不可用
    sb.s_free_inodes_count = sb.s_inodes_count.saturating_sub(RESERVED_INODES);

    // 文件系统状态与错误处理（参考 lwext4 fill_sb）
    sb.s_state = Ext4Superblock::EXT4_VALID_FS;
    sb.s_errors = Ext4Superblock::EXT4_ERRORS_RO;

    // 创建者 OS / 版本号
    sb.s_creator_os = Ext4Superblock::EXT4_OS_LINUX;
    sb.s_rev_level = Ext4Superblock::EXT4_DYNAMIC_REV;

    // 特性标志
    sb.s_feature_compat = DEFAULT_FEATURE_COMPAT;
    sb.s_feature_incompat = DEFAULT_FEATURE_INCOMPAT;
    sb.s_feature_ro_compat = DEFAULT_FEATURE_RO_COMPAT;

    // 块组描述符大小
    sb.s_desc_size = layout.desc_size;
    // 预留的 GDT 块数（仅 mkfs 默认值，挂载时应相信磁盘中的值）
    sb.s_reserved_gdt_blocks = layout.reserved_gdt_blocks as u16;
    sb.s_checksum_type = if ext4_superblock_has_metadata_csum(&sb) {
        1
    } else {
        0
    };
    sb.update_checksum();

    sb
}

/// 构建未初始化的块组描述符 不管字节序
fn build_uninit_group_desc(
    sb: &Ext4Superblock,
    group_id: u32,
    layout: &FsLayoutInfo,
) -> Ext4GroupDesc {
    let mut desc = Ext4GroupDesc::default();

    // 通过工具函数统一计算该块组的布局
    let gl = cloc_group_layout(
        group_id,
        sb,
        layout.blocks_per_group,
        layout.inode_table_blocks,
        layout.group0_block_bitmap,
        layout.group0_inode_bitmap,
        layout.group0_inode_table,
        layout.gdt_blocks,
    );

    // 位图和 inode 表块号
    desc.bg_block_bitmap_lo = gl.group_blcok_bitmap_startblocks as u32;
    desc.bg_inode_bitmap_lo = gl.group_inode_bitmap_startblocks as u32;
    desc.bg_inode_table_lo = gl.group_inode_table_startblocks as u32;

    // 理论空闲块数：整组减去元数据块
    let used_meta = gl.metadata_blocks_in_group as u32;
    let free_blocks = layout.blocks_per_group.saturating_sub(used_meta);

    if group_id == 0 {
        // 组0 还需要扣掉保留 inode
        desc.bg_free_blocks_count_lo = free_blocks as u16;
        desc.bg_free_inodes_count_lo =
            layout.inodes_per_group.saturating_sub(RESERVED_INODES) as u16;
        desc.bg_itable_unused_lo = layout.inodes_per_group.saturating_sub(RESERVED_INODES) as u16;
    } else {
        desc.bg_free_blocks_count_lo = free_blocks as u16;
        desc.bg_free_inodes_count_lo = layout.inodes_per_group as u16;
        desc.bg_itable_unused_lo = layout.inodes_per_group as u16;
    }

    // 目前不使用高 16 位计数和 UNINIT 标志
    desc.bg_free_blocks_count_hi = 0;
    desc.bg_free_inodes_count_hi = 0;
    desc.bg_used_dirs_count_lo = 0;
    desc.bg_used_dirs_count_hi = 0;
    desc.bg_flags = 0;

    desc
}

///写备份超级块到所有组，从块组1开始
fn write_superblock_redundant_backup<B: BlockDevice>(
    block_dev: &mut Jbd2Dev<B>,
    sb: &Ext4Superblock,
    groups_count: u32,
    fs_layout: &FsLayoutInfo,
) -> Ext4Result<()> {
    //从1开始
    // sparse_superbllock特性判断
    let sprse_feature =
        sb.has_feature_ro_compat(Ext4Superblock::EXT4_FEATURE_RO_COMPAT_SPARSE_SUPER);
    if sprse_feature {
        for gid in 1..groups_count {
            let group_layout = cloc_group_layout(
                gid,
                sb,
                fs_layout.blocks_per_group,
                fs_layout.inode_table_blocks,
                fs_layout.group0_block_bitmap,
                fs_layout.group0_inode_bitmap,
                fs_layout.group0_inode_table,
                fs_layout.gdt_blocks,
            );
            //需要超级块备份
            if need_redundant_backup(gid) {
                let super_blocks = group_layout.group_start_block;
                block_dev
                    .read_block(AbsoluteBN::new(super_blocks))
                    .expect("Superblock read failed!");
                let buffer = block_dev.buffer_mut();
                sb.to_disk_bytes(&mut buffer[0..SUPERBLOCK_SIZE]);
                block_dev.write_block(AbsoluteBN::new(super_blocks), true)?;
            }
        }
    }
    Ok(())
}

/// 写入超级块到磁盘 管字节序 不写备份
pub(crate) fn write_superblock<B: BlockDevice>(
    block_dev: &mut Jbd2Dev<B>,
    sb: &Ext4Superblock,
) -> Ext4Result<()> {
    // 超级块总是从分区偏移 1024 字节开始，占用 1024 字节
    if BLOCK_SIZE == 1024 {
        block_dev.read_block(AbsoluteBN::from(1u32))?;
        let buffer = block_dev.buffer_mut();
        sb.to_disk_bytes(&mut buffer[0..SUPERBLOCK_SIZE]);
        block_dev.write_block(AbsoluteBN::from(1u32), true)?;
    } else {
        block_dev.read_block(AbsoluteBN::from(0u32))?;
        let buffer = block_dev.buffer_mut();
        let offset = Ext4Superblock::SUPERBLOCK_OFFSET as usize; // 1024
        let end = offset + Ext4Superblock::SUPERBLOCK_SIZE;
        sb.to_disk_bytes(&mut buffer[offset..end]);
        block_dev.write_block(AbsoluteBN::from(0u32), false)?; //由于目前日志回放在超级块读取后，目前为了快速修复防止读取到旧的超级块。直接让超级块落盘写回
    }

    Ok(())
}

/// 读取超级块 管字节序
pub(crate) fn read_superblock<B: BlockDevice>(
    block_dev: &mut Jbd2Dev<B>,
) -> Ext4Result<Ext4Superblock> {
    // 超级块总是从分区偏移 1024 字节开始，占用 1024 字节
    // 这里通过按 BLOCK_SIZE 读块，再在块内做 1024 字节切片来解析
    if BLOCK_SIZE == 1024 {
        block_dev.read_block(AbsoluteBN::from(1u32))?;
        let buffer = block_dev.buffer();
        let sb = Ext4Superblock::from_disk_bytes(&buffer[0..SUPERBLOCK_SIZE]);
        Ok(sb)
    } else {
        block_dev.read_block(AbsoluteBN::from(0u32))?;
        let buffer = block_dev.buffer();
        let offset = Ext4Superblock::SUPERBLOCK_OFFSET as usize; // 1024
        let end = offset + Ext4Superblock::SUPERBLOCK_SIZE;
        let sb = Ext4Superblock::from_disk_bytes(&buffer[offset..end]);
        Ok(sb)
    }
}

///写入所有组的冗余备份中 自动判断特性
fn write_gdt_redundant_backup<B: BlockDevice>(
    block_dev: &mut Jbd2Dev<B>,
    descs: &VecDeque<Ext4GroupDesc>,
    sb: &Ext4Superblock,
    groups_count: u32,
    fs_layout: &FsLayoutInfo,
) -> Ext4Result<()> {
    //参数合法性判断
    let desc_size = sb.get_desc_size();
    let desc_all_size = descs.len() * desc_size as usize;
    let can_recive_size = fs_layout.gdt_blocks * fs_layout.descs_per_block * desc_size as u32;
    if can_recive_size < desc_all_size as u32 {
        return Err(Ext4Error::buffer_too_small(
            can_recive_size as usize,
            desc_all_size,
        ));
    }

    let sprse_feature =
        sb.has_feature_ro_compat(Ext4Superblock::EXT4_FEATURE_RO_COMPAT_SPARSE_SUPER);
    if sprse_feature {
        //为每个块组执行
        for gid in 1..groups_count {
            if need_redundant_backup(gid) {
                let group_layout = cloc_group_layout(
                    gid,
                    sb,
                    fs_layout.blocks_per_group,
                    fs_layout.inode_table_blocks,
                    fs_layout.group0_block_bitmap,
                    fs_layout.group0_inode_bitmap,
                    fs_layout.group0_inode_table,
                    fs_layout.gdt_blocks,
                );
                let gdt_start = group_layout.group_start_block + 1; //跳过超级块

                let mut desc_iter = descs.iter();
                //循环写入desc
                for gdt_block_id in gdt_start..group_layout.group_blcok_bitmap_startblocks {
                    block_dev.read_block(AbsoluteBN::new(gdt_block_id))?;
                    let buffer = block_dev.buffer_mut();
                    let mut current_offset = 0_usize; //descoffset循环记录
                    for _ in 0..fs_layout.descs_per_block {
                        if let Some(desc) = desc_iter.next() {
                            desc.to_disk_bytes(
                                &mut buffer[current_offset..current_offset + desc_size as usize],
                            );
                            current_offset += desc_size as usize;
                        }
                    }
                    //写回磁盘
                    block_dev.write_block(AbsoluteBN::new(gdt_block_id), true)?;
                }
            }
        }
    }

    Ok(())
}

/// 写入块组0的描述符 管字节序
fn write_group_desc<B: BlockDevice>(
    block_dev: &mut Jbd2Dev<B>,
    group_id: u32,
    desc: &mut Ext4GroupDesc,
) -> Ext4Result<()> {
    // 读取超级块以确定块组描述符大小
    let superblock = read_superblock(block_dev)?;
    let desc_size = superblock.get_desc_size() as usize;

    // GDT 基地址统一为块号 1 的起始字节偏移：按字节偏移计算所在块和块内偏移
    let gdt_base: u64 = BLOCK_SIZE as u64;
    let byte_offset = gdt_base + group_id as u64 * desc_size as u64;
    let block_size_u64 = BLOCK_SIZE as u64;
    let block_num = byte_offset / block_size_u64;
    let in_block = (byte_offset % block_size_u64) as usize;
    let end = in_block + desc_size;

    let inode_bitmap_blk = desc.inode_bitmap() as u32;
    block_dev.read_block(inode_bitmap_blk.into())?;
    let inode_bitmap_bytes = block_dev.buffer().to_vec();
    let block_bitmap_blk = desc.block_bitmap() as u32;
    block_dev.read_block(block_bitmap_blk.into())?;
    let block_bitmap_bytes = block_dev.buffer().to_vec();
    desc.update_checksum(
        &superblock,
        group_id,
        Some(&block_bitmap_bytes),
        Some(&inode_bitmap_bytes),
    );

    block_dev.read_block(AbsoluteBN::new(block_num))?;
    let buffer = block_dev.buffer_mut();
    if end > buffer.len() {
        return Err(Ext4Error::corrupted());
    }
    desc.to_disk_bytes(&mut buffer[in_block..end]);
    block_dev.write_block(AbsoluteBN::new(block_num), true)?;

    Ok(())
}

/// 初始化块组0
fn initialize_group_0<B: BlockDevice>(
    block_dev: &mut Jbd2Dev<B>,
    layout: &FsLayoutInfo,
) -> Ext4Result<()> {
    // 计算块组0的布局
    let block_bitmap_blk = layout.group0_block_bitmap;
    let inode_bitmap_blk = layout.group0_inode_bitmap;
    let inode_table_blk = layout.group0_inode_table;

    {
        let buffer = block_dev.buffer_mut();
        buffer.fill(0);
        // 标记元数据块为已使用：块0(引导) + 块1(超级块) + GDT + 块位图 + inode位图 + inode表
        let used_metadata_blocks = layout.group0_metadata_blocks as usize;
        for i in 0..used_metadata_blocks {
            let byte_idx = i / 8;
            let bit_idx = i % 8;
            buffer[byte_idx] |= 1 << bit_idx;
        }
    }
    block_dev.write_block(block_bitmap_blk.into(), true)?;

    {
        let buffer = block_dev.buffer_mut();
        buffer.fill(0);
        // 标记前 RESERVED_INODES 个inode为已使用（保留inode 1-10）
        for i in 0..RESERVED_INODES {
            let byte_idx = (i / 8) as usize;
            let bit_idx = i % 8;
            buffer[byte_idx] |= 1 << bit_idx;
        }

        // 2.5padding无效inode为1
        let bits_per_group = BLOCK_SIZE_U32 * 8;
        for i in layout.inodes_per_group..bits_per_group {
            let byte_idx: usize = (i / 8) as usize;
            let bit_idx = i % 8;
            buffer[byte_idx] |= 1 << bit_idx;
        }
    }
    block_dev.write_block(inode_bitmap_blk.into(), true)?;

    //  清零inode表
    {
        let buffer = block_dev.buffer_mut();
        buffer.fill(0);
    }
    for i in 0..layout.inode_table_blocks {
        block_dev.write_block((inode_table_blk + i).into(), true)?;
    }

    //  更新块组0的描述符（清除UNINIT标志）
    let mut desc = Ext4GroupDesc {
        bg_flags: Ext4GroupDesc::EXT4_BG_INODE_ZEROED,
        bg_free_blocks_count_lo: layout
            .blocks_per_group
            .saturating_sub(layout.group0_metadata_blocks) as u16,
        bg_free_inodes_count_lo: layout.inodes_per_group.saturating_sub(RESERVED_INODES) as u16,
        bg_itable_unused_lo: layout.inodes_per_group.saturating_sub(RESERVED_INODES) as u16,
        bg_block_bitmap_lo: block_bitmap_blk,
        bg_inode_bitmap_lo: inode_bitmap_blk,
        bg_inode_table_lo: inode_table_blk,
        ..Default::default()
    };

    write_group_desc(block_dev, 0, &mut desc)?;

    Ok(())
}

/// 初始化除块组0之外的所有块组的位图
/// 对于未使用任何块/ inode 的块组，位图全部清零，free_counts 等于整组容量
fn initialize_other_groups_bitmaps<B: BlockDevice>(
    block_dev: &mut Jbd2Dev<B>,
    layout: &FsLayoutInfo,
    sb: &Ext4Superblock,
) -> Ext4Result<()> {
    // 从块组1开始，逐组初始化
    for group_id in 1..layout.groups {
        // 使用与 build_uninit_group_desc 相同的布局计算
        let gl = cloc_group_layout(
            group_id,
            sb,
            layout.blocks_per_group,
            layout.inode_table_blocks,
            layout.group0_block_bitmap,
            layout.group0_inode_bitmap,
            layout.group0_inode_table,
            layout.gdt_blocks,
        );

        let block_bitmap_blk = gl.group_blcok_bitmap_startblocks as u32;
        let inode_bitmap_blk = gl.group_inode_bitmap_startblocks as u32;

        //  初始化块位图：全0 → 所有块空闲
        {
            let buffer = block_dev.buffer_mut();
            buffer.fill(0);
            // 标记元数据块已用（包括备份 superblock/GDT、位图和 inode 表）
            let used_blocks = gl.metadata_blocks_in_group as usize;
            for i in 0..used_blocks {
                let byte_idx = i / 8;
                let bit_idx = i % 8;
                buffer[byte_idx] |= 1 << bit_idx;
            }
        }
        block_dev.write_block(block_bitmap_blk.into(), true)?;

        {
            //  初始化inode位图：全0 → 所有inode空闲
            let buffer = block_dev.buffer_mut();
            buffer.fill(0);

            // padding无效inode
            let bits_per_group = BLOCK_SIZE_U32 * 8;
            for i in layout.inodes_per_group..bits_per_group {
                let byte_idx: usize = (i / 8) as usize;
                let bit_idx = i % 8;
                buffer[byte_idx] |= 1 << bit_idx;
            }
        }
        block_dev.write_block(inode_bitmap_blk.into(), true)?;
    }

    Ok(())
}
