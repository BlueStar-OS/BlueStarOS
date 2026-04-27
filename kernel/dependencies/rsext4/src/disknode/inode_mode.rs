use super::*;

impl Ext4Inode {
    pub const S_IFMT: u16 = 0xF000; // 文件类型位掩码
    pub const S_IFSOCK: u16 = 0xC000; // 套接字
    pub const S_IFLNK: u16 = 0xA000; // 符号链接
    pub const S_IFREG: u16 = 0x8000; // 普通文件
    pub const S_IFBLK: u16 = 0x6000; // 块设备
    pub const S_IFDIR: u16 = 0x4000; // 目录
    pub const S_IFCHR: u16 = 0x2000; // 字符设备
    pub const S_IFIFO: u16 = 0x1000; // FIFO
}

impl Ext4Inode {
    pub const S_ISUID: u16 = 0x0800; // 设置UID位
    pub const S_ISGID: u16 = 0x0400; // 设置GID位
    pub const S_ISVTX: u16 = 0x0200; // 粘滞位
    pub const S_IRWXU: u16 = 0x01C0; // 所有者权限掩码
    pub const S_IRUSR: u16 = 0x0100; // 所有者读权限
    pub const S_IWUSR: u16 = 0x0080; // 所有者写权限
    pub const S_IXUSR: u16 = 0x0040; // 所有者执行权限
    pub const S_IRWXG: u16 = 0x0038; // 组权限掩码
    pub const S_IRGRP: u16 = 0x0020; // 组读权限
    pub const S_IWGRP: u16 = 0x0010; // 组写权限
    pub const S_IXGRP: u16 = 0x0008; // 组执行权限
    pub const S_IRWXO: u16 = 0x0007; // 其他用户权限掩码
    pub const S_IROTH: u16 = 0x0004; // 其他用户读权限
    pub const S_IWOTH: u16 = 0x0002; // 其他用户写权限
    pub const S_IXOTH: u16 = 0x0001; // 其他用户执行权限
}
