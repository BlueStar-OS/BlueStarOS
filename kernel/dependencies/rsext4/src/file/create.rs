use super::blocks::build_file_block_mapping;
use super::*;

pub fn create_symbol_link<B: BlockDevice>(
    device: &mut Jbd2Dev<B>,
    fs: &mut Ext4FileSystem,
    src_path: &str,
    dst_path: &str,
) -> Ext4Result<()> {
    // 首先判断两个目标文件是否存在，被链接不存在报错，链接文件存在报错。
    let src_norm = split_paren_child_and_tranlatevalid(src_path);
    let dst_norm = split_paren_child_and_tranlatevalid(dst_path);

    if get_file_inode(fs, device, &src_norm)?.is_none() {
        return Err(Ext4Error::invalid_input());
    }
    if get_file_inode(fs, device, &dst_norm)?.is_some() {
        return Err(Ext4Error::invalid_input());
    }

    // 拆 parent / child（父目录必须存在）
    let (parent, child) = if let Some(pos) = dst_norm.rfind('/') {
        let p = if pos == 0 {
            "/".to_string()
        } else {
            dst_norm[..pos].to_string()
        };
        let c = dst_norm[pos + 1..].to_string();
        (p, c)
    } else {
        ("/".to_string(), dst_norm)
    };

    let (parent_ino_num, parent_inode) =
        match get_inode_with_num(fs, device, &parent).ok().flatten() {
            Some(v) => v,
            None => return Err(Ext4Error::invalid_input()),
        };
    if !parent_inode.is_dir() {
        return Err(Ext4Error::invalid_input());
    }

    // 为新链接分配 inode
    let new_ino = fs.alloc_inode(device)?;

    let target_bytes = src_path.as_bytes();
    let target_len = target_bytes.len();
    let size_lo = (target_len as u64 & 0xffffffff) as u32;
    let size_hi = ((target_len as u64) >> 32) as u32;
    let symlink_mode = Ext4Inode::S_IFLNK | 0o777;

    let mut new_inode = Ext4Inode::empty_for_reuse(fs.default_inode_extra_isize());
    new_inode.i_links_count = 1;
    new_inode.i_size_lo = size_lo;
    new_inode.i_size_high = size_hi;
    new_inode.i_flags = Ext4Inode::mask_flags_for_mode(
        symlink_mode,
        parent_inode.i_flags & Ext4Inode::EXT4_FL_INHERITED,
    );

    if target_len == 0 {
        new_inode.i_blocks_lo = 0;
        new_inode.l_i_blocks_high = 0;
        new_inode.i_block = [0; 15];
    } else if target_len <= 60 {
        // fast symlink：目标路径直接写入 i_block
        let mut raw = [0u8; 60];
        raw[..target_len].copy_from_slice(target_bytes);
        for i in 0..15 {
            new_inode.i_block[i] =
                u32::from_le_bytes([raw[i * 4], raw[i * 4 + 1], raw[i * 4 + 2], raw[i * 4 + 3]]);
        }
        new_inode.i_blocks_lo = 0;
        new_inode.l_i_blocks_high = 0;
    } else {
        // 普通 symlink：用数据块存储目标路径
        let mut data_blocks: Vec<AbsoluteBN> = Vec::new();
        let mut remaining = target_len;
        let mut src_off = 0usize;

        while remaining > 0 {
            if !fs.superblock.has_extents() && data_blocks.len() >= 12 {
                return Err(Ext4Error::unsupported());
            }

            let blk = fs.alloc_block(device)?;
            let write_len = core::cmp::min(remaining, BLOCK_SIZE);
            fs.datablock_cache.modify_new(device, blk, |data| {
                for b in data.iter_mut() {
                    *b = 0;
                }
                let end = src_off + write_len;
                data[..write_len].copy_from_slice(&target_bytes[src_off..end]);
            })?;

            data_blocks.push(blk);
            remaining -= write_len;
            src_off += write_len;
        }

        let used_datablocks = data_blocks.len() as u64;
        let iblocks_used = used_datablocks.saturating_mul(BLOCK_SIZE as u64 / 512) as u32;
        new_inode.i_blocks_lo = iblocks_used;
        new_inode.l_i_blocks_high = 0; // iblocks_used is u32, so high part is 0

        build_file_block_mapping(fs, &mut new_inode, &data_blocks, device);
    }

    let mut create_update = Ext4InodeMetadataUpdate::create(symlink_mode);
    if fs
        .superblock
        .has_feature_ro_compat(Ext4Superblock::EXT4_FEATURE_RO_COMPAT_PROJECT)
        && parent_inode.i_flags & Ext4Inode::EXT4_PROJINHERIT_FL != 0
    {
        create_update.projid = Some(parent_inode.i_projid);
    }

    fs.finalize_inode_update(device, new_ino, &mut new_inode, create_update)?;

    // 插入父目录目录项（symlink 类型）
    let mut parent_inode_copy = parent_inode;
    insert_dir_entry(
        fs,
        device,
        parent_ino_num,
        &mut parent_inode_copy,
        new_ino,
        &child,
        Ext4DirEntry2::EXT4_FT_SYMLINK,
    )?;

    Ok(())
}

/// Create a file entry, creating missing parent directories on demand.
pub fn mkfile<B: BlockDevice>(
    device: &mut Jbd2Dev<B>,
    fs: &mut Ext4FileSystem,
    path: &str,
    initial_data: Option<&[u8]>,
    file_type: Option<u8>,
) -> Ext4Result<Ext4Inode> {
    // 规范化路径
    let norm_path = split_paren_child_and_tranlatevalid(path);
    if norm_path.is_empty() || norm_path == "/" {
        return Err(Ext4Error::invalid_input());
    }

    // 如果目标已存在，直接报错
    if get_file_inode(fs, device, &norm_path)?.is_some() {
        return Err(Ext4Error::already_exists());
    }

    // 拆 parent / child
    let mut valid_path = norm_path;
    let split_point = match valid_path.rfind('/') {
        Some(v) => v,
        None => return Err(Ext4Error::invalid_input()),
    };
    let child = valid_path.split_off(split_point)[1..].to_string();
    if child.is_empty() {
        return Err(Ext4Error::invalid_input());
    }
    let parent = if valid_path.is_empty() {
        "/".to_string()
    } else {
        valid_path
    };

    // 确保父目录存在
    ensure_directory(device, fs, &parent)?;

    // 重新获取父目录 inode 及其 inode 号
    let (parent_ino_num, parent_inode) =
        get_inode_with_num(fs, device, &parent)?.ok_or(Ext4Error::not_found())?;

    //为新文件分配 inode（内部自动选择块组）
    let new_file_ino = fs.alloc_inode(device)?;

    // 如有初始数据，为文件分配一个或多个数据块并写入
    let mut data_blocks: Vec<AbsoluteBN> = Vec::new();
    let mut total_written: usize = 0;
    if let Some(buf) = initial_data {
        let mut remaining = buf.len();
        let mut src_off = 0usize;

        while remaining > 0 {
            // 如果未启用 extents，则最多只使用 12 个直接块
            if !fs.superblock.has_extents() && data_blocks.len() >= 12 {
                break;
            }

            let blk = match fs.alloc_block(device) {
                Ok(b) => b,
                Err(e) => {
                    error!("mkfile alloc_block failed path={path} err={e:?} ({e})");
                    break;
                }
            };

            let write_len = core::cmp::min(remaining, BLOCK_SIZE);

            // 将数据写入新分配的数据块，其余部分填零
            fs.datablock_cache.modify_new(device, blk, |data| {
                for b in data.iter_mut() {
                    *b = 0;
                }
                let end = src_off + write_len;
                data[..write_len].copy_from_slice(&buf[src_off..end]);
            })?;

            data_blocks.push(blk);
            total_written += write_len;
            remaining -= write_len;
            src_off += write_len;
        }
    }

    // 构造新文件 inode 的内存版本，然后通过 modify_inode 一次性写回
    let mut new_inode = Ext4Inode::empty_for_reuse(fs.default_inode_extra_isize());
    let imode = if let Some(ft) = file_type {
        match ft {
            Ext4DirEntry2::EXT4_FT_SYMLINK => Ext4Inode::S_IFLNK | 0o777,
            Ext4DirEntry2::EXT4_FT_REG_FILE => Ext4Inode::S_IFREG | 0o644,
            Ext4DirEntry2::EXT4_FT_DIR => Ext4Inode::S_IFDIR | 0o755,
            Ext4DirEntry2::EXT4_FT_BLKDEV => Ext4Inode::S_IFBLK | 0o600,
            Ext4DirEntry2::EXT4_FT_CHRDEV => Ext4Inode::S_IFCHR | 0o600,
            Ext4DirEntry2::EXT4_FT_FIFO => Ext4Inode::S_IFIFO | 0o644,
            Ext4DirEntry2::EXT4_FT_SOCK => Ext4Inode::S_IFSOCK | 0o644,
            _ => Ext4Inode::S_IFREG | 0o644,
        }
    } else {
        Ext4Inode::S_IFREG | 0o644
    };

    new_inode.i_flags =
        Ext4Inode::mask_flags_for_mode(imode, parent_inode.i_flags & Ext4Inode::EXT4_FL_INHERITED);

    //extend是否开启
    if fs.superblock.has_extents() {
        new_inode.write_extend_header();
    }

    new_inode.i_links_count = 1;

    let size_lo = (total_written & 0xffffffff) as u32;
    let size_hi = ((total_written as u64) >> 32) as u32;

    if !data_blocks.is_empty() {
        // 有初始数据：多块或单块文件
        let used_databyte = data_blocks.len() as u64;
        let iblocks_used = used_databyte.saturating_mul(BLOCK_SIZE as u64 / 512);
        let used_blocks_lo = iblocks_used as u32;
        new_inode.i_size_lo = size_lo;
        new_inode.i_size_high = size_hi;
        new_inode.i_blocks_lo = used_blocks_lo;
        new_inode.l_i_blocks_high = (iblocks_used >> 32) as u16;

        build_file_block_mapping(fs, &mut new_inode, &data_blocks, device);
    } else {
        //无初始数据：空文件
        new_inode.i_size_lo = 0;
        new_inode.i_size_high = 0;
        new_inode.i_blocks_lo = 0;
        new_inode.l_i_blocks_high = 0;
        if fs.superblock.has_extents() {
            new_inode.i_flags |= Ext4Inode::EXT4_EXTENTS_FL;
            new_inode.write_extend_header();
        } else {
            new_inode.i_block = [0; 15];
        }
    }

    let mut create_update = Ext4InodeMetadataUpdate::create(imode);
    if fs
        .superblock
        .has_feature_ro_compat(Ext4Superblock::EXT4_FEATURE_RO_COMPAT_PROJECT)
        && parent_inode.i_flags & Ext4Inode::EXT4_PROJINHERIT_FL != 0
    {
        create_update.projid = Some(parent_inode.i_projid);
    }

    fs.finalize_inode_update(device, new_file_ino, &mut new_inode, create_update)?;

    //在父目录中插入一个普通文件类型的目录项（必要时自动扩展目录块）

    let file_type = match file_type {
        Some(ft) => ft,
        None => Ext4DirEntry2::EXT4_FT_REG_FILE,
    };

    let mut parent_inode_copy = parent_inode;
    if insert_dir_entry(
        fs,
        device,
        parent_ino_num,
        &mut parent_inode_copy,
        new_file_ino,
        &child,
        file_type,
    )
    .is_err()
    {
        error!(
            "mkfile insert_dir_entry failed path={path} parent_ino={parent_ino_num} child={child} ino={new_file_ino}"
        );
        return Err(Ext4Error::corrupted());
    }

    // 返回新文件 inode
    fs.get_inode_by_num(device, new_file_ino)
}
