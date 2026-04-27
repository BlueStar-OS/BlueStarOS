use super::*;

/// 挂载 Ext4 文件系统
///
/// # 参数
///
/// * `dev` - 可变引用的块设备
///
/// # 返回值
/// 返回 `Ext4FileSystem` 实例或错误
///
/// # 参数
/// - `dev`: 块设备
pub fn fs_mount<B: BlockDevice>(dev: &mut Jbd2Dev<B>) -> Ext4Result<Ext4FileSystem> {
    ext4::mount(dev)
}

/// 卸载 Ext4 文件系统
///
/// # 参数
///
/// * `fs` - 文件系统实例
/// * `dev` - 可变引用的块设备
///
/// # 返回值
///
/// 成功时返回 `Ok(())`，失败时返回错误
pub fn fs_umount<B: BlockDevice>(fs: Ext4FileSystem, dev: &mut Jbd2Dev<B>) -> Ext4Result<()> {
    ext4::umount(fs, dev)
}
