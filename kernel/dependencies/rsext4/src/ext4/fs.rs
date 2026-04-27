use super::*;

/// Ext4 文件系统实例
///
/// 管理挂载后的文件系统状态，包含文件系统的所有核心数据结构
///
/// # 字段
///
/// * `superblock` - 文件系统超级块，包含文件系统元数据
/// * `group_descs` - 块组描述符数组
/// * `block_allocator` - 块分配器，管理数据块的分配和释放
/// * `inode_allocator` - Inode分配器，管理inode的分配和释放
/// * `bitmap_cache` - 位图缓存，按需加载，使用LRU淘汰策略
/// * `inodetable_cahce` - Inode表缓存
/// * `datablock_cache` - 数据块缓存
/// * `root_inode` - 根目录inode号
/// * `group_count` - 块组数量
/// * `mounted` - 是否已挂载标志
/// * `journal_sb_block_start` - Journal 超级块起始块号
pub struct Ext4FileSystem {
    /// 超级块
    pub superblock: Ext4Superblock,
    /// 块组描述符数组
    pub group_descs: Vec<Ext4GroupDesc>,
    /// 块分配器
    pub block_allocator: BlockAllocator,
    /// Inode分配器
    pub inode_allocator: InodeAllocator,
    /// 位图缓存（按需加载，LRU淘汰）
    pub bitmap_cache: BitmapCache,
    /// InodeTable缓存
    pub inodetable_cahce: InodeCache,
    /// DataBlock缓存
    pub datablock_cache: DataBlockCache,
    /// 根目录inode号
    pub root_inode: InodeNumber,
    /// 块组数量
    pub group_count: u32,
    /// 是否已挂载
    pub mounted: bool,
    /// Journal 超级块 开始块号
    pub journal_sb_block_start: Option<AbsoluteBN>,
}

impl Ext4FileSystem {
    pub(crate) fn sync_group_descriptor_if_needed<B: BlockDevice>(
        &mut self,
        block_dev: &mut Jbd2Dev<B>,
        group_id: BGIndex,
    ) -> Ext4Result<()> {
        if USE_MULTILEVEL_CACHE {
            return Ok(());
        }

        let idx = group_id.as_usize()?;
        if idx >= self.group_descs.len() {
            return Err(Ext4Error::corrupted());
        }

        let desc_size = self.superblock.get_desc_size() as usize;
        let gdt_base = BLOCK_SIZE as u64;
        let byte_offset = gdt_base + idx as u64 * desc_size as u64;
        let block_num = AbsoluteBN::new(byte_offset / BLOCK_SIZE as u64);
        let in_block = (byte_offset % BLOCK_SIZE as u64) as usize;
        let end = in_block + desc_size;

        let mut desc = self.group_descs[idx];
        desc.update_checksum(&self.superblock, group_id.raw(), None, None);
        self.group_descs[idx] = desc;

        let mut raw_desc_bytes = [0u8; Ext4GroupDesc::EXT4_DESC_SIZE_64BIT];
        desc.to_disk_bytes(&mut raw_desc_bytes);

        block_dev.read_block(block_num)?;
        let buffer = block_dev.buffer_mut();
        if end > buffer.len() {
            return Err(Ext4Error::corrupted());
        }

        buffer[in_block..end].copy_from_slice(&raw_desc_bytes[..desc_size]);
        block_dev.write_block(block_num, true)?;
        Ok(())
    }

    /// 检查指定inode是否已经被分配
    ///
    /// # 参数
    ///
    /// * `device` - 可变引用的块设备
    /// * `inode_num` - 要检查的inode号
    ///
    /// # 返回值
    ///
    /// 如果inode已分配返回`true`，否则返回`false`
    pub fn inode_num_already_allocted<B: BlockDevice>(
        &mut self,
        device: &mut Jbd2Dev<B>,
        inode_num: InodeNumber,
    ) -> bool {
        let (group_idx, inode_in_group) = match self.inode_allocator.global_to_group(inode_num) {
            Ok(ids) => ids,
            Err(_) => return false,
        };
        let desc = match group_idx.as_usize().ok().and_then(|idx| self.group_descs.get(idx)) {
            Some(d) => d,
            None => {
                warn!(
                    "inode_num_already_allocted: invalid group_idx {group_idx} for inode {inode_num}"
                );
                return false;
            }
        };
        let bitmap_block = AbsoluteBN::new(desc.inode_bitmap());
        let cache_key = CacheKey::new_inode(group_idx);

        let bitmap = match self
            .bitmap_cache
            .get_or_load_mut(device, cache_key, bitmap_block)
        {
            Ok(b) => b,
            Err(e) => {
                warn!("inode_num_already_allocted: load inode bitmap failed: {e:?}");
                return false;
            }
        };

        let bm = InodeBitmap::new(&mut bitmap.data, self.superblock.s_inodes_per_group);
        match bm.is_allocated(inode_in_group.raw()) {
            Some(allocated) => allocated,
            None => {
                warn!("inode_num_already_allocted: inode_in_group {inode_in_group} out of range");
                false
            }
        }
    }

    /// 获取块组描述符
    pub fn get_group_desc(&self, group_idx: BGIndex) -> Option<&Ext4GroupDesc> {
        group_idx
            .as_usize()
            .ok()
            .and_then(|idx| self.group_descs.get(idx))
    }

    /// 获取可变块组描述符
    pub fn get_group_desc_mut(&mut self, group_idx: BGIndex) -> Option<&mut Ext4GroupDesc> {
        group_idx
            .as_usize()
            .ok()
            .and_then(|idx| self.group_descs.get_mut(idx))
    }

    /// 使用闭包修改指定 inode，内部自动计算 inode 在磁盘上的位置
    pub fn modify_inode<B, F>(
        &mut self,
        block_dev: &mut Jbd2Dev<B>,
        inode_num: InodeNumber,
        f: F,
    ) -> Ext4Result<()>
    where
        B: BlockDevice,
        F: FnOnce(&mut Ext4Inode),
    {
        // 通过全局 inode 号计算所属块组
        let (group_idx, _idx_in_group) = self.inode_allocator.global_to_group(inode_num)?;

        let inode_table_start = self
            .group_descs
            .get(group_idx.as_usize()?)
            .ok_or(Ext4Error::corrupted())?
            .inode_table();

        let (block_num, offset, _g) = self.inodetable_cahce.calc_inode_location(
            inode_num,
            self.superblock.s_inodes_per_group,
            AbsoluteBN::new(inode_table_start),
            BLOCK_SIZE,
        )?;

        let sb = self.superblock;
        let inode_size = self.inode_disk_size() as usize;
        let has_csum = ext4_superblock_has_metadata_csum(&sb);

        let wrapped_f = move |inode: &mut Ext4Inode| {
            f(inode);
            if has_csum {
                ext4_update_inode_checksum(&sb, inode_num, inode.i_generation, inode, inode_size);
            }
        };

        self.inodetable_cahce
            .modify(block_dev, inode_num, block_num, offset, wrapped_f)
    }

    /// 按 inode 号加载 inode（只读），内部自动计算在磁盘上的位置
    pub fn get_inode_by_num<B: BlockDevice>(
        &mut self,
        block_dev: &mut Jbd2Dev<B>,
        inode_num: InodeNumber,
    ) -> Ext4Result<Ext4Inode> {
        let (group_idx, _idx_in_group) = self.inode_allocator.global_to_group(inode_num)?;

        let inode_table_start = self
            .group_descs
            .get(group_idx.as_usize()?)
            .ok_or(Ext4Error::corrupted())?
            .inode_table();

        let (block_num, offset, _g) = self.inodetable_cahce.calc_inode_location(
            inode_num,
            self.superblock.s_inodes_per_group,
            AbsoluteBN::new(inode_table_start),
            BLOCK_SIZE,
        )?;

        let cached =
            self.inodetable_cahce
                .get_or_load(block_dev, inode_num, block_num, offset)?;
        Ok(cached.inode)
    }

    /// 获取文件系统统计信息
    pub fn statfs(&self) -> FileSystemStats {
        FileSystemStats {
            total_blocks: self.superblock.blocks_count(),
            free_blocks: self.superblock.free_blocks_count(),
            total_inodes: self.superblock.s_inodes_count,
            free_inodes: self.superblock.s_free_inodes_count,
            block_size: self.superblock.block_size(),
            block_groups: self.group_count,
        }
    }

    ///创建最基本的file
    pub fn make_base_dir(&self) {
        //root journal lost+found
    }
}

/// 文件系统统计信息
#[derive(Debug, Clone, Copy)]
pub struct FileSystemStats {
    /// 总块数
    pub total_blocks: u64,
    /// 空闲块数
    pub free_blocks: u64,
    /// 总inode数
    pub total_inodes: u32,
    /// 空闲inode数
    pub free_inodes: u32,
    /// 块大小（字节）
    pub block_size: u64,
    /// 块组数
    pub block_groups: u32,
}
