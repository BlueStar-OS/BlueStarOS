use super::*;

/// Ext4 磁盘 Inode 结构
///
/// Inode 是文件系统中存储文件元数据的核心数据结构
/// 每个文件和目录都有一个对应的 inode
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Ext4Inode {
    // 0x00 - 基本文件属性
    pub i_mode: u16,        // 文件模式（类型和权限）
    pub i_uid: u16,         // 所有者用户ID（低16位）
    pub i_size_lo: u32,     // 文件大小（低32位，字节）
    pub i_atime: u32,       // 访问时间（秒）
    pub i_ctime: u32,       // 状态改变时间（秒）
    pub i_mtime: u32,       // 修改时间（秒）
    pub i_dtime: u32,       // 删除时间（秒）
    pub i_gid: u16,         // 所有者组ID（低16位）
    pub i_links_count: u16, // 硬链接计数
    pub i_blocks_lo: u32,   // 块计数（低32位）
    pub i_flags: u32,       // 文件标志

    // 0x20 - 操作系统相关字段1
    pub l_i_version: u32, // Linux：inode版本（用于NFS）

    // 0x24 - 数据块指针（60字节）
    pub i_block: [u32; 15], // 块指针数组或extent树

    // 0x64 - 文件版本和属性
    pub i_generation: u32,  // 文件版本（用于NFS）
    pub i_file_acl_lo: u32, // 扩展属性块（低32位）
    pub i_size_high: u32,   // 文件大小（高32位）或目录ACL
    pub i_obso_faddr: u32,  // 废弃的碎片地址

    // 0x74 - 操作系统相关字段2（12字节）
    pub l_i_blocks_high: u16,   // 块计数（高16位）
    pub l_i_file_acl_high: u16, // 扩展属性块（高16位）
    pub l_i_uid_high: u16,      // 所有者用户ID（高16位）
    pub l_i_gid_high: u16,      // 所有者组ID（高16位）
    pub l_i_checksum_lo: u16,   // inode校验和（低16位）
    pub l_i_reserved: u16,      // 保留字段

    // 0x80 - 扩展字段（对于大inode）
    pub i_extra_isize: u16,  // 额外inode大小
    pub i_checksum_hi: u16,  // inode校验和（高16位）
    pub i_ctime_extra: u32,  // 额外的状态改变时间（纳秒 + epoch高2位）
    pub i_mtime_extra: u32,  // 额外的修改时间（纳秒 + epoch高2位）
    pub i_atime_extra: u32,  // 额外的访问时间（纳秒 + epoch高2位）
    pub i_crtime: u32,       // 创建时间（秒）
    pub i_crtime_extra: u32, // 额外的创建时间（纳秒 + epoch高2位）
    pub i_version_hi: u32,   // 版本号（高32位）
    pub i_projid: u32,       // 项目ID
}

impl Ext4Inode {
    /// 写入初始extend header便捷函数
    pub fn write_extend_header(&mut self) {
        let per_extent_header_offset = Ext4ExtentHeader::disk_size();
        let current_offset = 0;
        let mut extent_buffer: [u8; 60] = [0; 60];
        let header = Ext4ExtentHeader::new();
        //写入header
        header.to_disk_bytes(
            &mut extent_buffer[current_offset..current_offset + per_extent_header_offset],
        );
        //转换写回
        let mut new_slice: [u32; 15] = [0; 15];
        for idx in 0..15 {
            new_slice[idx] = u32::from_le_bytes([
                extent_buffer[idx * 4],
                extent_buffer[idx * 4 + 1],
                extent_buffer[idx * 4 + 2],
                extent_buffer[idx * 4 + 3],
            ])
        }
        self.i_block.copy_from_slice(&new_slice);
    }

    /// 标准inode大小（128字节）
    pub const GOOD_OLD_INODE_SIZE: u16 = 128;

    /// 大inode默认大小（256字节）
    pub const LARGE_INODE_SIZE: u16 = 256;

    pub const EXT4_EPOCH_BITS: u32 = 2;
    pub const EXT4_EPOCH_MASK: u32 = 0x3;
    pub const EXT4_NSEC_MASK: u32 = !Self::EXT4_EPOCH_MASK;

    pub const FIELD_END_I_CHECKSUM_HI: u16 = 132;
    pub const FIELD_END_I_CTIME_EXTRA: u16 = 136;
    pub const FIELD_END_I_MTIME_EXTRA: u16 = 140;
    pub const FIELD_END_I_ATIME_EXTRA: u16 = 144;
    pub const FIELD_END_I_CRTIME: u16 = 148;
    pub const FIELD_END_I_CRTIME_EXTRA: u16 = 152;
    pub const FIELD_END_I_VERSION_HI: u16 = 156;
    pub const FIELD_END_I_PROJID: u16 = 160;

    /// 获取完整的文件大小（64位）
    pub fn size(&self) -> u64 {
        (self.i_size_high as u64) << 32 | self.i_size_lo as u64
    }

    /// 获取完整的块数（48位）
    pub fn blocks_count(&self) -> u64 {
        (self.l_i_blocks_high as u64) << 32 | self.i_blocks_lo as u64
    }

    /// 获取完整的UID（32位）
    pub fn uid(&self) -> u32 {
        (self.l_i_uid_high as u32) << 16 | self.i_uid as u32
    }

    /// 获取完整的GID（32位）
    pub fn gid(&self) -> u32 {
        (self.l_i_gid_high as u32) << 16 | self.i_gid as u32
    }

    pub fn set_uid(&mut self, uid: u32) {
        self.i_uid = (uid & 0xFFFF) as u16;
        self.l_i_uid_high = ((uid >> 16) & 0xFFFF) as u16;
    }

    pub fn set_gid(&mut self, gid: u32) {
        self.i_gid = (gid & 0xFFFF) as u16;
        self.l_i_gid_high = ((gid >> 16) & 0xFFFF) as u16;
    }

    /// 获取完整的扩展属性块号（48位）
    pub fn file_acl(&self) -> u64 {
        (self.l_i_file_acl_high as u64) << 32 | self.i_file_acl_lo as u64
    }

    /// 检查是否是目录
    pub fn is_dir(&self) -> bool {
        self.i_mode & Self::S_IFMT == Self::S_IFDIR
    }

    /// 检查是否是普通文件
    pub fn is_file(&self) -> bool {
        self.i_mode & Self::S_IFMT == Self::S_IFREG
    }

    /// 检查是否是符号链接
    pub fn is_symlink(&self) -> bool {
        self.i_mode & Self::S_IFMT == Self::S_IFLNK
    }

    pub fn permissions(&self) -> u16 {
        self.i_mode & !Self::S_IFMT
    }

    pub fn set_mode_preserve_type(&mut self, mode: u16) {
        self.i_mode = (self.i_mode & Self::S_IFMT) | (mode & !Self::S_IFMT);
    }

    pub fn set_mode_full(&mut self, mode: u16) {
        self.i_mode = mode;
    }

    pub fn is_executable(&self) -> bool {
        self.i_mode & (Self::S_IXUSR | Self::S_IXGRP | Self::S_IXOTH) != 0
    }

    pub fn clear_setid_bits_for_content_change(&mut self) {
        self.i_mode &= !Self::S_ISUID;
        if self.is_executable() {
            self.i_mode &= !Self::S_ISGID;
        }
    }

    pub fn clear_setid_bits_for_chown(&mut self) {
        self.i_mode &= !(Self::S_ISUID | Self::S_ISGID);
    }

    /// 检查是否使用extent树
    fn is_extent(&self) -> bool {
        self.i_flags & Self::EXT4_EXTENTS_FL != 0
    }
    ///检查是否有extend树的结构
    /// 检查EXT4_EXTENTS_FL标志和标志extend头
    pub fn have_extend_header_and_use_extend(&self) -> bool {
        if !Self::is_extent(self) {
            debug!("Inode not have extend flag!");
            return false;
        }

        let word0_le = self.i_block[0].to_le_bytes();
        let magic = u16::from_le_bytes([word0_le[0], word0_le[1]]);
        if magic == Ext4ExtentHeader::EXT4_EXT_MAGIC {
            true
        } else {
            debug!("No tree header!!!");
            false
        }
    }

    //some metadata change support
    pub fn set_mtime(&mut self, mtime: u32) {
        self.i_mtime = mtime;
    }
    pub fn set_ctime(&mut self, ctime: u32) {
        self.i_ctime = ctime;
    }
    pub fn set_atime(&mut self, atime: u32) {
        self.i_atime = atime;
    }

    pub fn max_extra_isize(inode_size: u16) -> u16 {
        inode_size.saturating_sub(Self::GOOD_OLD_INODE_SIZE)
    }

    pub fn required_extra_isize(field_end: u16) -> u16 {
        field_end.saturating_sub(Self::GOOD_OLD_INODE_SIZE)
    }

    pub fn field_fits(&self, inode_size: u16, field_end: u16) -> bool {
        field_end <= inode_size && field_end <= Self::GOOD_OLD_INODE_SIZE + self.i_extra_isize
    }

    fn encode_extra_time(ts: Ext4Timestamp) -> u32 {
        let ts = Ext4Timestamp::new(ts.sec, ts.nsec);
        let lower = ts.sec as i32 as i64;
        let extra = (((ts.sec - lower) >> 32) as u32) & Self::EXT4_EPOCH_MASK;
        extra | (ts.nsec << Self::EXT4_EPOCH_BITS)
    }

    fn decode_extra_time(base: u32, extra: u32) -> Ext4Timestamp {
        let mut sec = (base as i32) as i64;
        if (extra & Self::EXT4_EPOCH_MASK) != 0 {
            sec += ((extra & Self::EXT4_EPOCH_MASK) as i64) << 32;
        }
        let nsec = (extra & Self::EXT4_NSEC_MASK) >> Self::EXT4_EPOCH_BITS;
        Ext4Timestamp::new(sec, nsec)
    }

    fn encode_time_base(ts: Ext4Timestamp, fits_extra: bool) -> u32 {
        let sec = if fits_extra {
            ts.sec
        } else {
            ts.sec.clamp(i32::MIN as i64, i32::MAX as i64)
        };
        (sec as i32) as u32
    }

    fn get_raw_xtime(
        &self,
        inode_size: u16,
        field_end: u16,
        sec_field: u32,
        extra_field: u32,
    ) -> Ext4Timestamp {
        if self.field_fits(inode_size, field_end) {
            Self::decode_extra_time(sec_field, extra_field)
        } else {
            Ext4Timestamp::new((sec_field as i32) as i64, 0)
        }
    }

    pub fn set_atime_ts(&mut self, inode_size: u16, ts: Ext4Timestamp) {
        let fits_extra = self.field_fits(inode_size, Self::FIELD_END_I_ATIME_EXTRA);
        self.i_atime = Self::encode_time_base(ts, fits_extra);
        self.i_atime_extra = if fits_extra {
            Self::encode_extra_time(ts)
        } else {
            0
        };
    }

    pub fn set_mtime_ts(&mut self, inode_size: u16, ts: Ext4Timestamp) {
        let fits_extra = self.field_fits(inode_size, Self::FIELD_END_I_MTIME_EXTRA);
        self.i_mtime = Self::encode_time_base(ts, fits_extra);
        self.i_mtime_extra = if fits_extra {
            Self::encode_extra_time(ts)
        } else {
            0
        };
    }

    pub fn set_ctime_ts(&mut self, inode_size: u16, ts: Ext4Timestamp) {
        let fits_extra = self.field_fits(inode_size, Self::FIELD_END_I_CTIME_EXTRA);
        self.i_ctime = Self::encode_time_base(ts, fits_extra);
        self.i_ctime_extra = if fits_extra {
            Self::encode_extra_time(ts)
        } else {
            0
        };
    }

    pub fn set_crtime_ts(&mut self, inode_size: u16, ts: Ext4Timestamp) {
        if !self.field_fits(inode_size, Self::FIELD_END_I_CRTIME) {
            self.i_crtime = 0;
            self.i_crtime_extra = 0;
            return;
        }

        self.i_crtime = Self::encode_time_base(
            ts,
            self.field_fits(inode_size, Self::FIELD_END_I_CRTIME_EXTRA),
        );
        self.i_crtime_extra = if self.field_fits(inode_size, Self::FIELD_END_I_CRTIME_EXTRA) {
            Self::encode_extra_time(ts)
        } else {
            0
        };
    }

    pub fn atime_ts(&self, inode_size: u16) -> Ext4Timestamp {
        self.get_raw_xtime(
            inode_size,
            Self::FIELD_END_I_ATIME_EXTRA,
            self.i_atime,
            self.i_atime_extra,
        )
    }

    pub fn mtime_ts(&self, inode_size: u16) -> Ext4Timestamp {
        self.get_raw_xtime(
            inode_size,
            Self::FIELD_END_I_MTIME_EXTRA,
            self.i_mtime,
            self.i_mtime_extra,
        )
    }

    pub fn ctime_ts(&self, inode_size: u16) -> Ext4Timestamp {
        self.get_raw_xtime(
            inode_size,
            Self::FIELD_END_I_CTIME_EXTRA,
            self.i_ctime,
            self.i_ctime_extra,
        )
    }

    pub fn crtime_ts(&self, inode_size: u16) -> Option<Ext4Timestamp> {
        if !self.field_fits(inode_size, Self::FIELD_END_I_CRTIME) {
            return None;
        }

        Some(
            if self.field_fits(inode_size, Self::FIELD_END_I_CRTIME_EXTRA) {
                Self::decode_extra_time(self.i_crtime, self.i_crtime_extra)
            } else {
                Ext4Timestamp::new((self.i_crtime as i32) as i64, 0)
            },
        )
    }

    pub fn empty_for_reuse(default_extra_isize: u16) -> Self {
        Self {
            i_extra_isize: default_extra_isize,
            ..Default::default()
        }
    }
}
