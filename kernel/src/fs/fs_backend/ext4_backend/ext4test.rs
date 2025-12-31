use crate::driver::VirtBlk;
use super::{Ext4BlockDevice, Ext4Fs};
use log::info;
use crate::fs::vfs::{
    vfs_mkdir,
    vfs_mkfile,
    vfs_mv,
    vfs_read_at,
    vfs_remove,
    vfs_rename,
    vfs_write_at,
};

use rsext4::{
    OpenFile,
    lseek as ext4_lseek,
    mkdir,
    mkfile,
    mv as ext4_mv,
    open as ext4_open,
    read_at as ext4_read_at,
    rename as ext4_rename,
    write_at as ext4_write_at,
};

/// 简单的 ext4 自测：挂载 -> mkdir/mkfile -> 写入/读取 -> mv/rename
///
/// 注意：需要底层块设备上已经有一个合法的 ext4 分区，
/// 否则 `Ext4Fs::mount` 可能会失败。
#[cfg(feature = "ext4")]
pub fn ext4_smoke_test() {
    info!("[ext4_test] start");

    // 初始化底层块设备和 Ext4Fs
    let raw_blk = VirtBlk::new();
    let block_dev = Ext4BlockDevice::new(raw_blk);
    let mut ext4fs = Ext4Fs::new(block_dev);

    use crate::fs::vfs::VfsFs;
    ext4fs.mount().expect("ext4 mount failed in test");

    {
        // 拿到 rsext4 内部 dev/fs
        let Ext4Fs { dev, fs } = &mut ext4fs;
        let fs_inner = fs.as_mut().expect("ext4 fs not mounted");

        // 1. 创建测试目录和文件
        let _ = mkdir(dev, fs_inner, "/ext4_test");
        let _ = mkfile(dev, fs_inner, "/ext4_test/hello.txt", None, None);

        // 2. 打开文件并写入数据
        let mut of: OpenFile = ext4_open(dev, fs_inner, "/ext4_test/hello.txt", false)
            .expect("open hello.txt failed");
        let payload: &[u8] = b"hello ext4";
        ext4_write_at(dev, fs_inner, &mut of, payload)
            .expect("write_at failed");

        // 3. 回到文件开头读取并校验
        ext4_lseek(&mut of, 0);
        let data = ext4_read_at(dev, fs_inner, &mut of, payload.len())
            .expect("read_at failed");
        assert_eq!(&data[..payload.len()], payload);

        // 4. mv 到新路径
        ext4_mv(fs_inner, dev, "/ext4_test/hello.txt", "/ext4_test/hello_moved.txt")
            .expect("mv failed");

        // 5. 在同一目录下 rename
        ext4_rename(
            dev,
            fs_inner,
            "/ext4_test/hello_moved.txt",
            "/ext4_test/hello_renamed.txt",
        )
        .expect("rename failed");
    }

    ext4fs.umount().expect("ext4 umount failed in test");

    info!("[ext4_test] done");
}

/// 使用 VFS 高层 API 的自测：要求 ROOTFS 已经初始化（RootFs::init_rootfs 已调用）。
///
/// 流程：
/// 1. mkdir /vfs_test
/// 2. mkfile /vfs_test/hello.txt
/// 3. write_at / read_at 校验内容
/// 4. mv 到新路径
/// 5. rename 修改同目录下名字
/// 6. remove 删除最终文件
#[cfg(feature = "ext4")]
pub fn vfs_api_smoke_test() {
    use alloc::string::String;

    use crate::fs::vfs::{OpenFlags, vfs_open};

    info!("[vfs_api_test] start");

    let dir = "/vfs_test";
    let file = "/vfs_test/hello.txt";
    let file_moved = "/vfs_test/hello_moved.txt";
    let file_renamed = "/vfs_test/hello_renamed.txt";

    // 1. 创建目录（如果已存在忽略错误）
    let _ = vfs_mkdir(dir);

    // 2. 创建文件
    vfs_mkfile(file).expect("vfs_mkfile failed");
    let fd = vfs_open(file, OpenFlags::RDWR).unwrap().fd;
    // 3. 写入数据
    let payload: &[u8] = b"hello vfs api";
    let written = vfs_write_at(&fd, 0, payload).expect("vfs_write_at failed");
    assert_eq!(written, payload.len());

    // 4. 读回校验
    let mut buf = [0u8; 32];
    let read_len = vfs_read_at(&fd, 0, &mut buf).expect("vfs_read_at failed");
    assert!(read_len >= payload.len());
    assert_eq!(&buf[..payload.len()], payload);

    // 5. mv 到新路径（全路径）
    vfs_mv(file, file_moved).expect("vfs_mv failed");

    // 6. rename：只改名，不改父目录
    vfs_rename(file_moved, "hello_renamed.txt").expect("vfs_rename failed");

    // 7. 删除最终文件
    vfs_remove(file_renamed).expect("vfs_remove failed");

    info!("[vfs_api_test] done");
}

