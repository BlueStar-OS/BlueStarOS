use super::delete::remove_inodeentry_from_parentdir;
use super::*;

/// Create a hard link.
pub fn link<B: BlockDevice>(
    fs: &mut Ext4FileSystem,
    block_dev: &mut Jbd2Dev<B>,
    link_path: &str,
    linked_path: &str,
) -> Ext4Result<()> {
    let link_norm = split_paren_child_and_tranlatevalid(link_path);
    let linked_norm = split_paren_child_and_tranlatevalid(linked_path);

    // 1.检查 被链接文件本身是否存在，不存在返回。
    let (target_ino, target_inode) = match get_file_inode(fs, block_dev, &linked_norm) {
        Ok(Some(v)) => v,
        Ok(None) => return Err(Ext4Error::not_found()),
        Err(e) => return Err(e),
    };

    // 1.5 不允许链接目录
    if target_inode.is_dir() {
        return Err(Ext4Error::permission_denied());
    }

    // 2.检查链接文件本身是否已经存在同名entry，存在返回
    if get_file_inode(fs, block_dev, &link_norm)
        .ok()
        .flatten()
        .is_some()
    {
        return Err(Ext4Error::already_exists());
    }

    // link_path 的父目录必须存在且是目录
    let (parent_path, child_name) = if let Some(pos) = link_norm.rfind('/') {
        let parent = if pos == 0 {
            "/".to_string()
        } else {
            link_norm[..pos].to_string()
        };
        let child = link_norm[pos + 1..].to_string();
        (parent, child)
    } else {
        ("/".to_string(), link_norm)
    };
    let (parent_ino, mut parent_inode) = match get_inode_with_num(fs, block_dev, &parent_path)
        .ok()
        .flatten()
    {
        Some(v) => v,
        None => return Err(Ext4Error::not_found()),
    };
    if !parent_inode.is_dir() {
        return Err(Ext4Error::not_dir());
    }

    // 3.复制目标entry（主要复制 file_type），插入到当前父目录（新名字）
    let (linked_parent_path, linked_child_name) = if let Some(pos) = linked_norm.rfind('/') {
        let parent = if pos == 0 {
            "/".to_string()
        } else {
            linked_norm[..pos].to_string()
        };
        let child = linked_norm[pos + 1..].to_string();
        (parent, child)
    } else {
        ("/".to_string(), linked_norm.clone())
    };

    let mut copied_ft: Option<u8> = None;
    if let Some((_lpino, mut lp_inode)) = get_inode_with_num(fs, block_dev, &linked_parent_path)
        .ok()
        .flatten()
        && let Ok(blocks) = resolve_inode_block_allextend(fs, block_dev, &mut lp_inode)
    {
        for &phys in blocks.values() {
            let cached = match fs.datablock_cache.get_or_load(block_dev, phys) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let data = &cached.data[..BLOCK_SIZE];
            let iter = DirEntryIterator::new(data);
            for (entry, _) in iter {
                if entry.inode == 0 {
                    continue;
                }
                if entry.name == linked_child_name.as_bytes() {
                    copied_ft = Some(entry.file_type);
                    break;
                }
            }
            if copied_ft.is_some() {
                break;
            }
        }
    }

    let file_type = copied_ft.unwrap_or_else(|| {
        if target_inode.is_file() {
            Ext4DirEntry2::EXT4_FT_REG_FILE
        } else if target_inode.is_symlink() {
            Ext4DirEntry2::EXT4_FT_SYMLINK
        } else {
            Ext4DirEntry2::EXT4_FT_UNKNOWN
        }
    });

    // insert_dir_entry 会根据 child_name 重新计算 name_len/rec_len（满足“更新名字和长度信息”）
    if insert_dir_entry(
        fs,
        block_dev,
        parent_ino,
        &mut parent_inode,
        target_ino,
        &child_name,
        file_type,
    )
    .is_err()
    {
        return Err(Ext4Error::corrupted());
    }

    // 4.更新目标inode的link+1，失败则回滚刚插入的目录项
    let new_links = target_inode.i_links_count.saturating_add(1);
    if fs
        .set_inode_links_count(block_dev, target_ino, new_links)
        .is_err()
    {
        let _ = remove_inodeentry_from_parentdir(fs, block_dev, &parent_path, &child_name);
        return Err(Ext4Error::corrupted());
    }

    Ok(())
}
