//! Fat32 file system
//!
//! Endianness: FAT32 on-disk fields are little-endian.

use alloc::string::String;
use crate::alloc::string::ToString;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::any::Any;
use alloc::vec;
use spin::Mutex;
use alloc::format;
use crate::fs::vfs::{File, LinuxDirent64, MountFs, OpenFlags, VfsFs, VfsFsError, VfsStat, VFS_DT_DIR, VFS_DT_REG};

fn le16(b: &[u8]) -> u16 {
    u16::from_le_bytes([b[0], b[1]])
}

fn le32(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

fn le32_full(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

// FAT32 BPB layout offsets (relative to boot sector start)
// 注意：这些偏移是「分区内 boot sector(第0扇区)」内部的字节偏移。
const BPB_BYTS_PER_SEC_OFF: usize = 11; // u16：每扇区字节数（常见 512）
const BPB_SEC_PER_CLUS_OFF: usize = 13; // u8：每簇扇区数（2 的幂：1/2/4/8/..）
const BPB_RSVD_SEC_CNT_OFF: usize = 14; // u16：保留区扇区数（含 boot sector/FSInfo/备份引导等）
const BPB_NUM_FATS_OFF: usize = 16; // u8：FAT 表份数（通常 2）
const BPB_ROOT_ENT_CNT_OFF: usize = 17; // u16：FAT12/16 的根目录项数；FAT32 必须为 0（用于 sanity check）
const BPB_TOT_SEC16_OFF: usize = 19; // u16：总扇区数（小分区用；若非 0 则 TotSec32 应为 0）
const BPB_TOT_SEC32_OFF: usize = 32; // u32：总扇区数（FAT32 常用）
const BPB_FAT_SZ16_OFF: usize = 22; // u16：每份 FAT 表扇区数（FAT12/16 用；FAT32 必须为 0）
const BPB_FAT_SZ32_OFF: usize = 36; // u32：每份 FAT 表扇区数（FAT32 使用）
const BPB_ROOT_CLUS_OFF: usize = 44; // u32：根目录起始簇号（FAT32 特有，常见为 2）
const BPB_FSINFO_OFF: usize = 48; // u16：FSInfo 扇区号（相对保留区起点，常见为 1）
const BPB_BK_BOOT_SEC_OFF: usize = 50; // u16：备份引导扇区号（相对保留区起点，常见为 6）

const FAT32_MIN_SECTOR_SIZE: usize = 512; // 最小扇区大小（实际值由 BPB.BytsPerSec 给出，但这里用于读 boot sector）

const DIR_ENTRY_SIZE: usize = 32;
const ATTR_LONG_NAME: u8 = 0x0F;
const ATTR_DIRECTORY: u8 = 0x10;
const ATTR_VOLUME_ID: u8 = 0x08;

const FAT32_EOC_MIN: u32 = 0x0FFFFFF8;
const FAT32_EOC: u32 = 0x0FFFFFFF;

pub struct Fat32Fs{
    pub dev: Arc<dyn File>, // vblock 分区设备（按“分区内偏移”读写：offset=0 表示分区第0字节）
    pub info: Fat32Info,     // 从 BPB/FSInfo 推导出来的几何与布局信息
    mounted: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Fat32SfnEntry {
    pub first_byte: u8,
    pub name11: [u8; 11],
    pub attr: u8,
    pub first_cluster: u32,
    pub size: u32,
}

impl Fat32SfnEntry {
    fn from_raw(raw: &[u8]) -> Result<Self, VfsFsError> {
        if raw.len() != DIR_ENTRY_SIZE {
            return Err(VfsFsError::Invalid);
        }
        let first_byte = raw[0];
        let mut name11 = [0u8; 11];
        name11.copy_from_slice(&raw[0..11]);
        let attr = raw[11];
        // FAT32 SFN: first cluster is split into HI(20..21) and LO(26..27)
        let hi = le16(&raw[20..22]) as u32;
        let lo = le16(&raw[26..28]) as u32;
        let first_cluster = (hi << 16) | lo;
        let size = le32(&raw[28..32]);
        Ok(Self { first_byte, name11, attr, first_cluster, size })
    }

    fn first_byte(&self) -> u8 {
        self.first_byte
    }

    fn attr(&self) -> u8 {
        self.attr
    }

    fn name11(&self) -> [u8; 11] {
        self.name11
    }

    fn first_cluster(&self) -> u32 {
        self.first_cluster
    }

    fn size(&self) -> u32 {
        self.size
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Fat32LfnEntry {
    pub ord: u8,
    pub attr: u8,
    pub entry_type: u8,
    pub checksum: u8,
    pub fst_clus_lo: u16,
    pub name_u16: [u16; 13],
}

impl Fat32LfnEntry {
    fn from_raw(raw: &[u8]) -> Result<Self, VfsFsError> {
        if raw.len() != DIR_ENTRY_SIZE {
            return Err(VfsFsError::Invalid);
        }
        let ord = raw[0];
        let attr = raw[11];
        let entry_type = raw[12];
        let checksum = raw[13];
        let fst_clus_lo = le16(&raw[26..28]);

        // Validate LFN entry invariants (FAT spec):
        // - attr must be 0x0F
        // - type must be 0
        // - fstClusLO must be 0
        if attr != ATTR_LONG_NAME || entry_type != 0 || fst_clus_lo != 0 {
            return Err(VfsFsError::Invalid);
        }

        // LFN name fragments (UTF-16LE), 13 code units total
        // Name1: bytes 1..10 (5 u16)
        // Name2: bytes 14..25 (6 u16)
        // Name3: bytes 28..31 (2 u16)
        let mut name_u16 = [0u16; 13];
        let mut idx = 0usize;
        for i in (1..11).step_by(2) {
            name_u16[idx] = u16::from_le_bytes([raw[i], raw[i + 1]]);
            idx += 1;
        }
        for i in (14..26).step_by(2) {
            name_u16[idx] = u16::from_le_bytes([raw[i], raw[i + 1]]);
            idx += 1;
        }
        for i in (28..32).step_by(2) {
            name_u16[idx] = u16::from_le_bytes([raw[i], raw[i + 1]]);
            idx += 1;
        }

        Ok(Self { ord, attr, entry_type, checksum, fst_clus_lo, name_u16 })
    }

    fn ord(&self) -> u8 {
        self.ord
    }

    fn seq(&self) -> u8 {
        self.ord & 0x1F
    }

    fn is_last(&self) -> bool {
        (self.ord & 0x40) != 0
    }

    fn checksum(&self) -> u8 {
        self.checksum
    }

    fn name_part_u16(&self) -> Vec<u16> {
        self.name_u16.to_vec()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Fat32Info {
    // --- raw fields from BPB ---
    pub byts_per_sec: u16, // BPB_BytsPerSec：每扇区字节数
    pub sec_per_clus: u8,  // BPB_SecPerClus：每簇扇区数（簇大小 = byts_per_sec * sec_per_clus）
    pub rsvd_sec_cnt: u16, // BPB_RsvdSecCnt：保留区扇区数
    pub num_fats: u8,      // BPB_NumFATs：FAT 表份数
    pub tot_sec: u32,      // BPB_TotSec16/32：分区总扇区数（合并后的结果）
    pub fat_sz: u32,       // BPB_FATSz16/32：每份 FAT 表扇区数（合并后的结果；FAT32 常用 fat_sz32）
    pub root_clus: u32,    // BPB_RootClus：根目录起始簇号（FAT32 根目录是普通目录文件）
    pub fsinfo_sec: u16,   // BPB_FSInfo：FSInfo 扇区号（相对分区起点的扇区号；常见 1）
    pub bk_boot_sec: u16,  // BPB_BkBootSec：备份引导扇区号（相对分区起点的扇区号；常见 6）

    // --- computed layout (all in units of sectors, relative to partition start) ---
    pub fat_lba0: u64,        // 第 1 份 FAT 的起始 LBA（分区内相对 LBA：fat_lba0=RsvdSecCnt）
    pub data_lba0: u64,       // 数据区起始 LBA（分区内相对 LBA：data_lba0=RsvdSecCnt+NumFATs*FATSz32）
    pub clus_bytes: u32,      // 每簇字节数 = byts_per_sec * sec_per_clus
    pub total_clusters: u32,  // 总簇数（粗略计算：data_secs / sec_per_clus；用于 sanity check/遍历边界）
}

impl Fat32Info {
    pub fn parse_from_boot_sector(sector: &[u8]) -> Result<Self, VfsFsError> {
        // sector：分区第 0 扇区（boot sector / VBR）原始 512B 数据。
        if sector.len() < FAT32_MIN_SECTOR_SIZE {
            return Err(VfsFsError::Invalid);
        }

        let byts_per_sec = le16(&sector[BPB_BYTS_PER_SEC_OFF..BPB_BYTS_PER_SEC_OFF + 2]);
        let sec_per_clus = sector[BPB_SEC_PER_CLUS_OFF];
        let rsvd_sec_cnt = le16(&sector[BPB_RSVD_SEC_CNT_OFF..BPB_RSVD_SEC_CNT_OFF + 2]);
        let num_fats = sector[BPB_NUM_FATS_OFF];
        let root_ent_cnt = le16(&sector[BPB_ROOT_ENT_CNT_OFF..BPB_ROOT_ENT_CNT_OFF + 2]);
        let tot_sec16 = le16(&sector[BPB_TOT_SEC16_OFF..BPB_TOT_SEC16_OFF + 2]);
        let tot_sec32 = le32(&sector[BPB_TOT_SEC32_OFF..BPB_TOT_SEC32_OFF + 4]);
        let fat_sz16 = le16(&sector[BPB_FAT_SZ16_OFF..BPB_FAT_SZ16_OFF + 2]);
        let fat_sz32 = le32(&sector[BPB_FAT_SZ32_OFF..BPB_FAT_SZ32_OFF + 4]);
        let root_clus = le32(&sector[BPB_ROOT_CLUS_OFF..BPB_ROOT_CLUS_OFF + 4]);
        let fsinfo_sec = le16(&sector[BPB_FSINFO_OFF..BPB_FSINFO_OFF + 2]);
        let bk_boot_sec = le16(&sector[BPB_BK_BOOT_SEC_OFF..BPB_BK_BOOT_SEC_OFF + 2]);

        if byts_per_sec == 0 || sec_per_clus == 0 || num_fats == 0 {
            return Err(VfsFsError::Invalid);
        }

        // FAT32 规范要求：RootEntCnt == 0 且 FATSz16 == 0。
        if root_ent_cnt != 0 || fat_sz16 != 0 {
            return Err(VfsFsError::Invalid);
        }

        let tot_sec = if tot_sec16 != 0 {
            tot_sec16 as u32
        } else {
            tot_sec32
        };
        let fat_sz = if fat_sz32 != 0 {
            fat_sz32
        } else {
            fat_sz16 as u32
        };

        if tot_sec == 0 || fat_sz == 0 {
            return Err(VfsFsError::Invalid);
        }

        // FAT 区 / 数据区布局（分区内相对 LBA）：
        // fat_lba0  = rsvd_sec_cnt
        // data_lba0 = rsvd_sec_cnt + num_fats * fat_sz
        let fat_lba0 = rsvd_sec_cnt as u64;
        let data_lba0 = rsvd_sec_cnt as u64 + (num_fats as u64) * (fat_sz as u64);

        let clus_bytes = (byts_per_sec as u32)
            .checked_mul(sec_per_clus as u32)
            .ok_or(VfsFsError::Invalid)?;

        // 总簇数（粗略）：data_sectors / sec_per_clus
        // data_sectors = tot_sec - rsvd_sec_cnt - num_fats*fat_sz
        let data_secs = tot_sec
            .saturating_sub(rsvd_sec_cnt as u32)
            .saturating_sub((num_fats as u32).saturating_mul(fat_sz));
        let total_clusters = if sec_per_clus == 0 {
            0
        } else {
            data_secs / (sec_per_clus as u32)
        };

        Ok(Self {
            byts_per_sec,
            sec_per_clus,
            rsvd_sec_cnt,
            num_fats,
            tot_sec,
            fat_sz,
            root_clus,
            fsinfo_sec,
            bk_boot_sec,
            fat_lba0,
            data_lba0,
            clus_bytes,
            total_clusters,
        })
    }

    pub fn clus_lba(&self, clus: u32) -> u64 {
        // FAT32：簇号从 2 开始（0/1 保留）。
        // cluster N 的起始 LBA = data_lba0 + (N - 2) * sec_per_clus
        if clus < 2 {
            self.data_lba0
        } else {
            self.data_lba0 + ((clus - 2) as u64) * (self.sec_per_clus as u64)
        }
    }
}

impl Fat32Fs {
    // 创建一个新的 FAT32 文件系统实例
    pub fn new(dev: Arc<dyn File>) -> Result<Self, VfsFsError> {
        // 读取分区第 0 扇区（boot sector/VBR），解析 BPB 得到布局信息。
        let mut sector = [0u8; FAT32_MIN_SECTOR_SIZE];
        dev.read_at(0, &mut sector)?;
        let info = Fat32Info::parse_from_boot_sector(&sector)?;
        Ok(Self { dev, info, mounted: false })
    }
}

fn align_up(x: usize, align: usize) -> usize {
    (x + align - 1) & !(align - 1)
}

fn make_sfn_name11(name: &str) -> Result<[u8; 11], VfsFsError> {
    if name.is_empty() {
        return Err(VfsFsError::Invalid);
    }
    let mut base = String::new();
    let mut ext = String::new();
    let mut seen_dot = false;
    for ch in name.chars() {
        if ch == '/' {
            return Err(VfsFsError::Invalid);
        }
        if ch == '.' {
            if seen_dot {
                return Err(VfsFsError::Invalid);
            }
            seen_dot = true;
            continue;
        }
        if !ch.is_ascii() {
            return Err(VfsFsError::NotSupported);
        }
        let up = (ch as u8).to_ascii_uppercase() as char;
        if up == ' ' {
            return Err(VfsFsError::Invalid);
        }
        if !seen_dot {
            base.push(up);
        } else {
            ext.push(up);
        }
    }
    if base.is_empty() || base.len() > 8 || ext.len() > 3 {
        return Err(VfsFsError::Invalid);
    }
    let mut out = [b' '; 11];
    out[0..base.len()].copy_from_slice(base.as_bytes());
    if !ext.is_empty() {
        out[8..8 + ext.len()].copy_from_slice(ext.as_bytes());
    }
    Ok(out)
}

fn split_parent(path: &str) -> Result<(String, String), VfsFsError> {
    if path.is_empty() {
        return Err(VfsFsError::Invalid);
    }
    let p = if path.ends_with('/') && path.len() > 1 {
        &path[..path.len() - 1]
    } else {
        path
    };
    let mut it = p.rsplitn(2, '/');
    let name = it.next().unwrap_or("");
    let parent = it.next().unwrap_or("");
    if name.is_empty() {
        return Err(VfsFsError::Invalid);
    }
    let parent_path = if parent.is_empty() { "/".to_string() } else { format!("/{}", parent) };
    Ok((parent_path, name.to_string()))
}

fn sfn_to_string(name11: &[u8; 11]) -> String {
    let base = &name11[0..8];
    let ext = &name11[8..11];

    let base_end = base.iter().position(|&b| b == b' ').unwrap_or(base.len());
    let ext_end = ext.iter().position(|&b| b == b' ').unwrap_or(ext.len());

    let mut s = String::new();
    for &b in &base[..base_end] {
        if b == 0 {
            break;
        }
        s.push(b as char);
    }
    if ext_end > 0 {
        s.push('.');
        for &b in &ext[..ext_end] {
            if b == 0 {
                break;
            }
            s.push(b as char);
        }
    }
    s
}

fn lfn_checksum(sfn11: &[u8; 11]) -> u8 {
    let mut sum: u8 = 0;
    for &b in sfn11.iter() {
        sum = ((sum & 1) << 7).wrapping_add(sum >> 1).wrapping_add(b);
    }
    sum
}


fn lfn_u16_to_string(u16s: &[u16]) -> String {
    let mut s = String::new();
    for &u in u16s.iter() {
        if u == 0x0000 || u == 0xFFFF {
            break;
        }
        if (0xD800..=0xDFFF).contains(&u) {
            s.push('?');
            continue;
        }
        match core::char::from_u32(u as u32) {
            Some(ch) => s.push(ch),
            None => s.push('?'),
        }
    }
    s
}

fn build_lfn_name(parts: &[(u8, Vec<u16>)]) -> String {
    if parts.is_empty() {
        return String::new();
    }
    let mut v: Vec<(u8, Vec<u16>)> = parts.to_vec();
    v.sort_by_key(|(seq, _)| *seq);
    let mut all: Vec<u16> = Vec::new();
    for (_, p) in v.iter() {
        all.extend_from_slice(p);
    }
    lfn_u16_to_string(&all)
}

fn name_matches(candidate: &str, target: &str) -> bool {
    if candidate.eq_ignore_ascii_case(target) {
        return true;
    }
    if candidate.is_ascii() && target.is_ascii() {
        return candidate.eq_ignore_ascii_case(target);
    }
    candidate == target
}

#[derive(Clone, Copy, Debug)]
struct DirEnt {
    attr: u8,
    first_clus: u32,
    size: u32,
    dirent_clus: u32,
    dirent_off: usize,
}

impl DirEnt {
    fn is_dir(&self) -> bool {
        (self.attr & ATTR_DIRECTORY) != 0
    }
}

impl Fat32Fs {


    fn sector_bytes(&self) -> usize {
        self.info.byts_per_sec as usize
    }

    fn read_sector(&self, lba: u64, out: &mut [u8]) -> Result<(), VfsFsError> {
        let off = (lba as usize)
            .checked_mul(self.sector_bytes())
            .ok_or(VfsFsError::Invalid)?;
        self.dev.read_at(off, out)?;
        Ok(())
    }

    fn write_sector(&self, lba: u64, data: &[u8]) -> Result<(), VfsFsError> {
        let off = (lba as usize)
            .checked_mul(self.sector_bytes())
            .ok_or(VfsFsError::Invalid)?;
        self.dev.write_at(off, data)?;
        Ok(())
    }

    fn read_fat_entry(&self, clus: u32) -> Result<u32, VfsFsError> {
        // FAT32：每项 4 字节（高 4 bit 保留，低 28 bit 有效）
        let byts_per_sec = self.info.byts_per_sec as u32;
        let off = (clus as u64) * 4;
        let sec = self.info.fat_lba0 + (off / byts_per_sec as u64);
        let ent_off = (off % byts_per_sec as u64) as usize;

        let mut buf = vec![0u8; self.sector_bytes()];
        self.read_sector(sec, &mut buf)?;
        if ent_off + 4 > buf.len() {
            return Err(VfsFsError::Invalid);
        }
        let v = le32_full(&buf[ent_off..ent_off + 4]) & 0x0FFFFFFF;
        Ok(v)
    }

    fn write_fat_entry(&self, clus: u32, val: u32) -> Result<(), VfsFsError> {
        let byts_per_sec = self.info.byts_per_sec as u32;
        let off = (clus as u64) * 4;
        let sec_off = (off / byts_per_sec as u64) as u64;
        let ent_off = (off % byts_per_sec as u64) as usize;
        let v = (val & 0x0FFFFFFF) | 0xF0000000;

        for fat_i in 0..(self.info.num_fats as u64) {
            let sec = self.info.fat_lba0 + fat_i * (self.info.fat_sz as u64) + sec_off;
            let mut buf = vec![0u8; self.sector_bytes()];
            self.read_sector(sec, &mut buf)?;
            buf[ent_off..ent_off + 4].copy_from_slice(&v.to_le_bytes());
            self.write_sector(sec, &buf)?;
        }
        Ok(())
    }

    fn next_cluster(&self, clus: u32) -> Result<Option<u32>, VfsFsError> {
        let v = self.read_fat_entry(clus)?;
        if v >= FAT32_EOC_MIN {
            return Ok(None);
        }
        if v == 0 {
            return Ok(None);
        }
        Ok(Some(v))
    }

    fn free_cluster_chain(&self, first_clus: u32) -> Result<(), VfsFsError> {
        if first_clus < 2 {
            return Ok(());
        }
        let mut cur = first_clus;
        loop {
            let next = self.next_cluster(cur)?;
            self.write_fat_entry(cur, 0)?;
            match next {
                Some(n) => cur = n,
                None => break,
            }
        }
        Ok(())
    }

    fn read_cluster(&self, clus: u32, out: &mut [u8]) -> Result<(), VfsFsError> {
        if out.len() != self.info.clus_bytes as usize {
            return Err(VfsFsError::Invalid);
        }
        let lba0 = self.info.clus_lba(clus);
        let sec_sz = self.sector_bytes();
        for i in 0..(self.info.sec_per_clus as usize) {
            let start = i * sec_sz;
            let end = start + sec_sz;
            self.read_sector(lba0 + i as u64, &mut out[start..end])?;
        }
        Ok(())
    }

    fn write_cluster(&self, clus: u32, data: &[u8]) -> Result<(), VfsFsError> {
        if data.len() != self.info.clus_bytes as usize {
            return Err(VfsFsError::Invalid);
        }
        let lba0 = self.info.clus_lba(clus);
        let sec_sz = self.sector_bytes();
        for i in 0..(self.info.sec_per_clus as usize) {
            let start = i * sec_sz;
            let end = start + sec_sz;
            self.write_sector(lba0 + i as u64, &data[start..end])?;
        }
        Ok(())
    }

    fn alloc_free_cluster(&self) -> Result<u32, VfsFsError> {
        let start = 2u32;
        let end = self.info.total_clusters.saturating_add(2);
        for c in start..end {
            if self.read_fat_entry(c)? == 0 {
                self.write_fat_entry(c, FAT32_EOC)?;
                let mut zero = vec![0u8; self.info.clus_bytes as usize];
                self.write_cluster(c, &zero)?;
                return Ok(c);
            }
        }
        Err(VfsFsError::NoSpace)
    }

    fn ensure_nth_cluster(&self, first: u32, nth: usize) -> Result<(u32, u32), VfsFsError> {
        if first == 0 {
            let new_first = self.alloc_free_cluster()?;
            if nth == 0 {
                return Ok((new_first, new_first));
            }
            let mut cur = new_first;
            for _ in 0..nth {
                let next = self.alloc_free_cluster()?;
                self.write_fat_entry(cur, next)?;
                cur = next;
            }
            return Ok((new_first, cur));
        }

        let mut cur = first;
        let mut idx = 0usize;
        while idx < nth {
            match self.next_cluster(cur)? {
                Some(n) => {
                    cur = n;
                    idx += 1;
                }
                None => {
                    let next = self.alloc_free_cluster()?;
                    self.write_fat_entry(cur, next)?;
                    cur = next;
                    idx += 1;
                }
            }
        }
        Ok((first, cur))
    }

    fn read_file_at(&self, first_clus: u32, size: u32, offset: usize, out: &mut [u8]) -> Result<usize, VfsFsError> {
        let file_size = size as usize;
        if offset >= file_size {
            return Ok(0);
        }
        let max_n = core::cmp::min(out.len(), file_size - offset);
        if max_n == 0 {
            return Ok(0);
        }

        let clus_bytes = self.info.clus_bytes as usize;
        let mut clus_idx = offset / clus_bytes;
        let mut inner = offset % clus_bytes;

        // walk to the cluster containing offset
        let mut clus = first_clus;
        while clus_idx > 0 {
            clus = self.next_cluster(clus)?.ok_or(VfsFsError::IO)?;
            clus_idx -= 1;
        }

        let mut tmp = vec![0u8; clus_bytes];
        let mut copied = 0usize;
        while copied < max_n {
            self.read_cluster(clus, &mut tmp)?;
            let can = core::cmp::min(max_n - copied, clus_bytes - inner);
            out[copied..copied + can].copy_from_slice(&tmp[inner..inner + can]);
            copied += can;
            inner = 0;
            if copied >= max_n {
                break;
            }
            clus = match self.next_cluster(clus)? {
                Some(n) => n,
                None => break,
            };
        }
        Ok(copied)
    }

    fn write_sfn_dirent(
        &self,
        dirent_clus: u32,
        dirent_off: usize,
        name11: [u8; 11],
        attr: u8,
        first_clus: u32,
        size: u32,
    ) -> Result<(), VfsFsError> {
        let mut buf = vec![0u8; self.info.clus_bytes as usize];
        self.read_cluster(dirent_clus, &mut buf)?;
        if dirent_off + DIR_ENTRY_SIZE > buf.len() {
            return Err(VfsFsError::Invalid);
        }
        let e = &mut buf[dirent_off..dirent_off + DIR_ENTRY_SIZE];
        for b in e.iter_mut() {
            *b = 0;
        }
        e[0..11].copy_from_slice(&name11);
        e[11] = attr;
        let hi = ((first_clus >> 16) as u16).to_le_bytes();
        let lo = ((first_clus & 0xFFFF) as u16).to_le_bytes();
        e[20..22].copy_from_slice(&hi);
        e[26..28].copy_from_slice(&lo);
        e[28..32].copy_from_slice(&size.to_le_bytes());
        self.write_cluster(dirent_clus, &buf)
    }

    fn find_free_dirent_slot(&self, dir_first: u32) -> Result<(u32, usize), VfsFsError> {
        let mut clus = dir_first;
        let mut buf = vec![0u8; self.info.clus_bytes as usize];
        loop {
            self.read_cluster(clus, &mut buf)?;
            for off in (0..buf.len()).step_by(DIR_ENTRY_SIZE) {
                let b0 = buf[off];
                if b0 == 0x00 || b0 == 0xE5 {
                    return Ok((clus, off));
                }
            }
            match self.next_cluster(clus)? {
                Some(n) => clus = n,
                None => {
                    let newc = self.alloc_free_cluster()?;
                    self.write_fat_entry(clus, newc)?;
                    self.write_fat_entry(newc, FAT32_EOC)?;
                    return Ok((newc, 0));
                }
            }
        }
    }

    fn find_free_dirent_slots(&self, dir_first: u32, need: usize) -> Result<(u32, usize), VfsFsError> {
        if need == 0 {
            return Err(VfsFsError::Invalid);
        }
        let mut clus = dir_first;
        let mut buf = vec![0u8; self.info.clus_bytes as usize];
        let entries_per_clus = buf.len() / DIR_ENTRY_SIZE;
        if need > entries_per_clus {
            return Err(VfsFsError::NotSupported);
        }
        loop {
            self.read_cluster(clus, &mut buf)?;
            let mut run = 0usize;
            let mut run_start_off = 0usize;
            for i in 0..entries_per_clus {
                let off = i * DIR_ENTRY_SIZE;
                let b0 = buf[off];
                let free = b0 == 0x00 || b0 == 0xE5;
                if free {
                    if run == 0 {
                        run_start_off = off;
                    }
                    run += 1;
                    if run >= need {
                        return Ok((clus, run_start_off));
                    }
                } else {
                    run = 0;
                }
            }
            match self.next_cluster(clus)? {
                Some(n) => clus = n,
                None => {
                    let newc = self.alloc_free_cluster()?;
                    self.write_fat_entry(clus, newc)?;
                    self.write_fat_entry(newc, FAT32_EOC)?;
                    return Ok((newc, 0));
                }
            }
        }
    }

    fn make_sfn_alias11(&self, long: &str) -> Result<[u8; 11], VfsFsError> {
        let mut out = [b' '; 11];
        let mut base: Vec<u8> = Vec::new();
        let mut ext: Vec<u8> = Vec::new();
        let mut in_ext = false;
        for ch in long.bytes() {
            if ch == b'.' {
                in_ext = true;
                continue;
            }
            let ok = (ch >= b'0' && ch <= b'9') || (ch >= b'a' && ch <= b'z') || (ch >= b'A' && ch <= b'Z') || ch == b'_';
            if !ok {
                continue;
            }
            let up = if ch >= b'a' && ch <= b'z' { ch - 32 } else { ch };
            if in_ext {
                if ext.len() < 3 {
                    ext.push(up);
                }
            } else {
                if base.len() < 8 {
                    base.push(up);
                }
            }
        }
        if base.is_empty() {
            return Err(VfsFsError::Invalid);
        }
        let take = core::cmp::min(6, base.len());
        out[0..take].copy_from_slice(&base[0..take]);
        out[6] = b'~';
        out[7] = b'1';
        for (i, b) in ext.iter().enumerate() {
            out[8 + i] = *b;
        }
        Ok(out)
    }

    fn write_lfn_dirent(&self, dirent_clus: u32, dirent_off: usize, ord: u8, checksum: u8, name13: &[u16; 13]) -> Result<(), VfsFsError> {
        let mut buf = vec![0u8; self.info.clus_bytes as usize];
        self.read_cluster(dirent_clus, &mut buf)?;
        if dirent_off + DIR_ENTRY_SIZE > buf.len() {
            return Err(VfsFsError::Invalid);
        }
        let e = &mut buf[dirent_off..dirent_off + DIR_ENTRY_SIZE];
        for b in e.iter_mut() {
            *b = 0;
        }
        e[0] = ord;
        e[11] = ATTR_LONG_NAME;
        e[12] = 0;
        e[13] = checksum;

        let mut put_u16 = |idx: usize, v: u16| {
            let b = v.to_le_bytes();
            e[idx] = b[0];
            e[idx + 1] = b[1];
        };
        for i in 0..5 {
            put_u16(1 + i * 2, name13[i]);
        }
        for i in 0..6 {
            put_u16(14 + i * 2, name13[5 + i]);
        }
        put_u16(26, 0);
        for i in 0..2 {
            put_u16(28 + i * 2, name13[11 + i]);
        }

        self.write_cluster(dirent_clus, &buf)
    }

    fn write_name_dirents(
        &self,
        parent_clus: u32,
        name: &str,
        attr: u8,
        first_clus: u32,
        size: u32,
    ) -> Result<(u32, usize, [u8; 11]), VfsFsError> {
        if let Ok(name11) = make_sfn_name11(name) {
            let (dclus, doff) = self.find_free_dirent_slot(parent_clus)?;
            self.write_sfn_dirent(dclus, doff, name11, attr, first_clus, size)?;
            return Ok((dclus, doff, name11));
        }

        let sfn11 = self.make_sfn_alias11(name)?;
        let checksum = lfn_checksum(&sfn11);
        let mut u16s: Vec<u16> = name.encode_utf16().collect();
        u16s.push(0u16);
        let lfn_cnt = (u16s.len() + 12) / 13;
        let total = lfn_cnt + 1;
        let (start_clus, start_off) = self.find_free_dirent_slots(parent_clus, total)?;

        for i in 0..lfn_cnt {
            let idx_from_end = lfn_cnt - 1 - i;
            let ord = (idx_from_end as u8) + 1;
            let ord = if idx_from_end == lfn_cnt - 1 { ord | 0x40 } else { ord };
            let mut part = [0xFFFFu16; 13];
            for j in 0..13 {
                let k = idx_from_end * 13 + j;
                if k < u16s.len() {
                    part[j] = u16s[k];
                    if u16s[k] == 0 {
                        for t in (j + 1)..13 {
                            part[t] = 0xFFFF;
                        }
                        break;
                    }
                }
            }
            let off = start_off + i * DIR_ENTRY_SIZE;
            self.write_lfn_dirent(start_clus, off, ord, checksum, &part)?;
        }

        let sfn_off = start_off + lfn_cnt * DIR_ENTRY_SIZE;
        self.write_sfn_dirent(start_clus, sfn_off, sfn11, attr, first_clus, size)?;
        Ok((start_clus, sfn_off, sfn11))
    }

    fn walk_dir_find(&self, dir_clus: u32, target: &str) -> Result<DirEnt, VfsFsError> {
        let mut clus = dir_clus;
        let mut buf = vec![0u8; self.info.clus_bytes as usize];
        let mut lfn_parts: Vec<(u8, Vec<u16>)> = Vec::new();
        let mut lfn_ck: Option<u8> = None;

        loop {
            self.read_cluster(clus, &mut buf)?;
            for off in (0..buf.len()).step_by(DIR_ENTRY_SIZE) {
                let e = &buf[off..off + DIR_ENTRY_SIZE];
                let sfn = Fat32SfnEntry::from_raw(e)?;
                let first = sfn.first_byte();
                if first == 0x00 {
                    return Err(VfsFsError::NotFound);
                }
                if first == 0xE5 {
                    lfn_parts.clear();
                    lfn_ck = None;
                    continue;
                }
                let attr = sfn.attr();
                if attr == ATTR_LONG_NAME {
                    let lfn = match Fat32LfnEntry::from_raw(e) {
                        Ok(v) => v,
                        Err(_) => {
                            lfn_parts.clear();
                            lfn_ck = None;
                            continue;
                        }
                    };
                    let seq = lfn.seq();
                    let ck = lfn.checksum();
                    if lfn.is_last() {
                        lfn_parts.clear();
                        lfn_ck = Some(ck);
                    }
                    if lfn_ck.is_some() && lfn_ck == Some(ck) {
                        lfn_parts.push((seq, lfn.name_part_u16()));
                    } else {
                        lfn_parts.clear();
                        lfn_ck = None;
                    }
                    continue;
                }
                if (attr & ATTR_VOLUME_ID) != 0 {
                    lfn_parts.clear();
                    lfn_ck = None;
                    continue;
                }

                let name11 = sfn.name11();
                let mut name = String::new();
                if !lfn_parts.is_empty() {
                    if let Some(expect) = lfn_ck {
                        if expect == lfn_checksum(&name11) {
                            name = build_lfn_name(&lfn_parts);
                        }
                    }
                }
                if name.is_empty() {
                    name = sfn_to_string(&name11);
                }
                lfn_parts.clear();
                lfn_ck = None;

                if name_matches(&name, target) {
                    let first_clus = sfn.first_cluster();
                    let size = sfn.size();
                    return Ok(DirEnt { attr, first_clus, size, dirent_clus: clus, dirent_off: off });
                }
            }

            match self.next_cluster(clus)? {
                Some(n) => clus = n,
                None => return Err(VfsFsError::NotFound),
            }
        }
    }

    fn open_path(&self, path: &str) -> Result<(u32, bool, u32), VfsFsError> {
        // returns (first_cluster, is_dir, size)
        if path == "/" || path.is_empty() {
            return Ok((self.info.root_clus, true, 0));
        }
        let comps: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let mut cur_clus = self.info.root_clus;
        for (idx, c) in comps.iter().enumerate() {
            let ent = self.walk_dir_find(cur_clus, c)?;
            let is_last = idx + 1 == comps.len();
            if is_last {
                return Ok((ent.first_clus, ent.is_dir(), ent.size));
            }
            if !ent.is_dir() {
                return Err(VfsFsError::NotDir);
            }
            cur_clus = ent.first_clus;
        }
        Ok((cur_clus, true, 0))
    }

    fn open_path_with_loc(&self, path: &str) -> Result<(u32, bool, u32, Option<(u32, usize)>, Option<[u8; 11]>), VfsFsError> {
        if path == "/" || path.is_empty() {
            return Ok((self.info.root_clus, true, 0, None, None));
        }
        let comps: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let mut cur_clus = self.info.root_clus;
        for (idx, c) in comps.iter().enumerate() {
            let ent = self.walk_dir_find(cur_clus, c)?;
            let is_last = idx + 1 == comps.len();
            if is_last {
                let mut clbuf = vec![0u8; self.info.clus_bytes as usize];
                self.read_cluster(ent.dirent_clus, &mut clbuf)?;
                let raw = &clbuf[ent.dirent_off..ent.dirent_off + DIR_ENTRY_SIZE];
                let sfn = Fat32SfnEntry::from_raw(raw)?;
                return Ok((ent.first_clus, ent.is_dir(), ent.size, Some((ent.dirent_clus, ent.dirent_off)), Some(sfn.name11())));
            }
            if !ent.is_dir() {
                return Err(VfsFsError::NotDir);
            }
            cur_clus = ent.first_clus;
        }
        Ok((cur_clus, true, 0, None, None))
    }

    fn write_file_at(
        &self,
        dirent_loc: Option<(u32, usize)>,
        sfn11: Option<[u8; 11]>,
        first_clus: &mut u32,
        size: &mut u32,
        offset: usize,
        data: &[u8],
    ) -> Result<usize, VfsFsError> {
        if data.is_empty() {
            return Ok(0);
        }
        let clus_bytes = self.info.clus_bytes as usize;
        let end_pos = offset.saturating_add(data.len());
        let need_clusters = if end_pos == 0 { 0 } else { (end_pos + clus_bytes - 1) / clus_bytes };
        if need_clusters == 0 {
            return Ok(0);
        }

        // Ensure cluster chain length.
        if *first_clus == 0 {
            let (nf, _) = self.ensure_nth_cluster(0, 0)?;
            *first_clus = nf;
        }
        let last_idx = need_clusters - 1;
        let (_, _) = self.ensure_nth_cluster(*first_clus, last_idx)?;

        // Write payload.
        let mut copied = 0usize;
        while copied < data.len() {
            let pos = offset + copied;
            let clus_idx = pos / clus_bytes;
            let inner = pos % clus_bytes;
            let (_, clus) = self.ensure_nth_cluster(*first_clus, clus_idx)?;

            let mut buf = vec![0u8; clus_bytes];
            self.read_cluster(clus, &mut buf)?;
            let can = core::cmp::min(data.len() - copied, clus_bytes - inner);
            buf[inner..inner + can].copy_from_slice(&data[copied..copied + can]);
            self.write_cluster(clus, &buf)?;
            copied += can;
        }

        // Update size and flush SFN dirent (if we know its location).
        let new_size = core::cmp::max(*size as usize, end_pos) as u32;
        *size = new_size;
        if let (Some((dclus, doff)), Some(name11)) = (dirent_loc, sfn11) {
            self.write_sfn_dirent(dclus, doff, name11, 0x20, *first_clus, *size)?;
        }
        Ok(copied)
    }

    fn dir_getdents(&self, dir_first: u32, start_off: u64, max_len: usize) -> Result<Vec<u8>, VfsFsError> {
        let mut stream: Vec<u8> = Vec::new();
        let mut clus = dir_first;
        let mut buf = vec![0u8; self.info.clus_bytes as usize];
        let hdr_len = core::mem::size_of::<LinuxDirent64>();
        let mut cur_off: u64 = 0;
        let mut lfn_parts: Vec<(u8, Vec<u16>)> = Vec::new();
        let mut lfn_ck: Option<u8> = None;

        loop {
            self.read_cluster(clus, &mut buf)?;
            for off in (0..buf.len()).step_by(DIR_ENTRY_SIZE) {
                let e = &buf[off..off + DIR_ENTRY_SIZE];
                let sfn = Fat32SfnEntry::from_raw(e)?;
                let first = sfn.first_byte();
                if first == 0x00 {
                    return Ok(stream);
                }
                if first == 0xE5 {
                    lfn_parts.clear();
                    lfn_ck = None;
                    continue;
                }
                let attr = sfn.attr();
                if attr == ATTR_LONG_NAME {
                    let lfn = match Fat32LfnEntry::from_raw(e) {
                        Ok(v) => v,
                        Err(_) => {
                            lfn_parts.clear();
                            lfn_ck = None;
                            continue;
                        }
                    };
                    let seq = lfn.seq();
                    let ck = lfn.checksum();
                    if lfn.is_last() {
                        lfn_parts.clear();
                        lfn_ck = Some(ck);
                    }
                    if lfn_ck.is_some() && lfn_ck == Some(ck) {
                        lfn_parts.push((seq, lfn.name_part_u16()));
                    } else {
                        lfn_parts.clear();
                        lfn_ck = None;
                    }
                    continue;
                }
                if (attr & ATTR_VOLUME_ID) != 0 {
                    lfn_parts.clear();
                    lfn_ck = None;
                    continue;
                }

                let name11 = sfn.name11();
                let mut name = String::new();
                if !lfn_parts.is_empty() {
                    if let Some(expect) = lfn_ck {
                        if expect == lfn_checksum(&name11) {
                            name = build_lfn_name(&lfn_parts);
                        }
                    }
                }
                if name.is_empty() {
                    name = sfn_to_string(&name11);
                }
                lfn_parts.clear();
                lfn_ck = None;
                if name.is_empty() {
                    continue;
                }

                let first_clus = sfn.first_cluster();
                let dtype = if (attr & ATTR_DIRECTORY) != 0 { VFS_DT_DIR } else { VFS_DT_REG };

                let name_bytes = name.as_bytes();
                let reclen = align_up(hdr_len + name_bytes.len() + 1, 8);
                let next_off = cur_off.saturating_add(reclen as u64);

                // We interpret file offset as the logical byte offset within the dirent stream.
                // Only emit entries when we have reached/passed start_off.
                if cur_off >= start_off {
                    if stream.len() + reclen > max_len {
                        return Ok(stream);
                    }
                    let base = stream.len();
                    stream.resize(base + reclen, 0);

                    let hdr = LinuxDirent64 {
                        d_ino: first_clus as u64,
                        d_off: next_off,
                        d_reclen: reclen as u16,
                        d_type: dtype as u8,
                    };
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            &hdr as *const _ as *const u8,
                            stream[base..].as_mut_ptr(),
                            hdr_len,
                        );
                    }
                    let name_off = base + hdr_len;
                    stream[name_off..name_off + name_bytes.len()].copy_from_slice(name_bytes);
                    stream[name_off + name_bytes.len()] = 0;
                }

                cur_off = next_off;
            }

            match self.next_cluster(clus)? {
                Some(n) => clus = n,
                None => break,
            }
        }
        Ok(stream)
    }
}

pub struct Fat32File {
    mount_fs: MountFs,
    first_clus: Mutex<u32>,
    size: Mutex<u32>,
    is_dir: bool,
    offset: Mutex<usize>,
    dirent_loc: Option<(u32, usize)>,
    sfn11: Option<[u8; 11]>,
}

impl Fat32File {
    fn with_fs<T>(&self, f: impl FnOnce(&Fat32Fs) -> Result<T, VfsFsError>) -> Result<T, VfsFsError> {
        let mut guard = self.mount_fs.lock();
        let fs = guard.as_any().downcast_ref::<Fat32Fs>().ok_or(VfsFsError::NotSupported)?;
        f(fs)
    }
}

impl File for Fat32File {
    

    fn read(&self, buf: &mut [u8]) -> Result<usize, VfsFsError> {
        let mut off = self.offset.lock();
        let n = self.read_at(*off, buf)?;
        *off += n;
        Ok(n)
    }

    fn write(&self, buf: &[u8]) -> Result<usize, VfsFsError> {
        let mut off = self.offset.lock();
        let n = self.write_at(*off, buf)?;
        *off += n;
        Ok(n)
    }

    fn read_at(&self, offset: usize, buf: &mut [u8]) -> Result<usize, VfsFsError> {
        if self.is_dir {
            return Err(VfsFsError::IsDir);
        }
        let first = *self.first_clus.lock();
        let size = *self.size.lock();
        self.with_fs(|fs| fs.read_file_at(first, size, offset, buf))
    }

    fn write_at(&self, offset: usize, buf: &[u8]) -> Result<usize, VfsFsError> {
        if self.is_dir {
            return Err(VfsFsError::IsDir);
        }
        let mut first = self.first_clus.lock();
        let mut size = self.size.lock();
        self.with_fs(|fs| fs.write_file_at(self.dirent_loc, self.sfn11, &mut *first, &mut *size, offset, buf))
    }

    fn lseek(&self, offset: isize, whence: usize) -> Result<usize, VfsFsError> {
        let mut off = self.offset.lock();
        let cur = *off as isize;
        let end = if self.is_dir { 0 } else { *self.size.lock() as isize };
        let new = match whence {
            0 => offset,
            1 => cur.saturating_add(offset),
            2 => end.saturating_add(offset),
            _ => return Err(VfsFsError::Invalid),
        };
        if new < 0 {
            return Err(VfsFsError::Invalid);
        }
        *off = new as usize;
        Ok(*off)
    }

    fn getdents64(&self, max_len: usize) -> Result<Vec<u8>, VfsFsError> {
        if !self.is_dir {
            return Err(VfsFsError::NotDir);
        }
        let mut off = self.offset.lock();
        let first = *self.first_clus.lock();
        let data = self.with_fs(|fs| fs.dir_getdents(first, *off as u64, max_len))?;
        *off = off.saturating_add(data.len());
        Ok(data)
    }

    fn stat(&self) -> Result<VfsStat, VfsFsError> {
        Ok(VfsStat {
            inode: *self.first_clus.lock(),
            size: if self.is_dir { 0 } else { *self.size.lock() as u64 },
            mode: 0,
            file_type: if self.is_dir { VFS_DT_DIR } else { VFS_DT_REG },
        })
    }
}

impl VfsFs for Fat32Fs {
    fn unlink(&mut self, _path: &str) -> Result<(), VfsFsError> {
        if !self.mounted {
            return Err(VfsFsError::Unmounted);
        }
        let path = _path;
        if path == "/" || path.is_empty() {
            return Err(VfsFsError::Invalid);
        }

        let (parent_path, name) = split_parent(path)?;
        let (pclus, is_dir, _, _, _) = self.open_path_with_loc(&parent_path)?;
        if !is_dir {
            return Err(VfsFsError::NotDir);
        }

        let ent = self.walk_dir_find(pclus, &name)?;
        if (ent.attr & ATTR_DIRECTORY) != 0 {
            return Err(VfsFsError::IsDir);
        }

        // Mark SFN entry deleted.
        let mut buf = vec![0u8; self.info.clus_bytes as usize];
        self.read_cluster(ent.dirent_clus, &mut buf)?;
        if ent.dirent_off + DIR_ENTRY_SIZE > buf.len() {
            return Err(VfsFsError::Invalid);
        }
        let raw = &buf[ent.dirent_off..ent.dirent_off + DIR_ENTRY_SIZE];
        let sfn = Fat32SfnEntry::from_raw(raw)?;
        let sfn11 = sfn.name11();
        let ck = lfn_checksum(&sfn11);

        buf[ent.dirent_off] = 0xE5;
        self.write_cluster(ent.dirent_clus, &buf)?;

        // Mark preceding LFN entries deleted (minimal: only within same cluster).
        let mut off = ent.dirent_off;
        while off >= DIR_ENTRY_SIZE {
            let prev = off - DIR_ENTRY_SIZE;
            let e = &buf[prev..prev + DIR_ENTRY_SIZE];
            if e[0] == 0x00 {
                break;
            }
            // LFN entry must have attr 0x0F and same checksum.
            if e[11] != ATTR_LONG_NAME || e[13] != ck {
                break;
            }
            buf[prev] = 0xE5;
            off = prev;
        }
        self.write_cluster(ent.dirent_clus, &buf)?;

        // Free data clusters.
        self.free_cluster_chain(ent.first_clus)?;
        Ok(())
    }

    fn mount(&mut self) -> Result<(), VfsFsError> {
        if self.mounted {
            return Err(VfsFsError::Mounted);
        }
        // 最小实现：验证根目录簇号在合法范围内
        if self.info.root_clus < 2 {
            return Err(VfsFsError::Invalid);
        }
        self.mounted = true;
        Ok(())
    }

    fn umount(&mut self) -> Result<(), VfsFsError> {
        if !self.mounted {
            return Err(VfsFsError::Unmounted);
        }
        self.mounted = false;
        Ok(())
    }

    fn name(&self) -> Result<String, VfsFsError> {
        Ok("fat32".into())
    }

    fn mkdir(&mut self, path: &str) -> Result<(), VfsFsError> {
        if !self.mounted {
            return Err(VfsFsError::Unmounted);
        }
        if path == "/" || path.is_empty() {
            return Ok(());
        }

        if self.open_path(path).is_ok() {
            return Err(VfsFsError::AlreadyExists);
        }

        let (parent_path, dir_name) = split_parent(path)?;
        let (pclus, is_dir, _, _, _) = self.open_path_with_loc(&parent_path)?;
        if !is_dir {
            return Err(VfsFsError::NotDir);
        }

        let new_clus = self.alloc_free_cluster()?;

        // Initialize directory cluster with "." and ".." entries.
        let mut dot = [b' '; 11];
        dot[0] = b'.';
        let mut dotdot = [b' '; 11];
        dotdot[0] = b'.';
        dotdot[1] = b'.';
        self.write_sfn_dirent(new_clus, 0, dot, ATTR_DIRECTORY, new_clus, 0)?;
        let parent_for_dotdot = if pclus == self.info.root_clus { self.info.root_clus } else { pclus };
        self.write_sfn_dirent(new_clus, DIR_ENTRY_SIZE, dotdot, ATTR_DIRECTORY, parent_for_dotdot, 0)?;

        let _ = self.write_name_dirents(pclus, &dir_name, ATTR_DIRECTORY, new_clus, 0)?;
        Ok(())
    }

    fn mkfile(&mut self, path: &str) -> Result<(), VfsFsError> {
        if !self.mounted {
            return Err(VfsFsError::Unmounted);
        }
        if path == "/" || path.is_empty() {
            return Err(VfsFsError::Invalid);
        }

        if self.open_path(path).is_ok() {
            return Err(VfsFsError::AlreadyExists);
        }

        let (parent_path, file_name) = split_parent(path)?;
        let (pclus, is_dir, _, _, _) = self.open_path_with_loc(&parent_path)?;
        if !is_dir {
            return Err(VfsFsError::NotDir);
        }

        let _ = self.write_name_dirents(pclus, &file_name, 0x20, 0, 0)?;
        Ok(())
    }

    fn open(&mut self, mount_fs: MountFs, path: &str, flags: OpenFlags) -> Result<Arc<dyn File>, VfsFsError> {
        if !self.mounted {
            return Err(VfsFsError::Unmounted);
        }

        // 读写能力说明：当前 FAT32 后端仍以「只读」为主。
        // - 只读打开：允许
        // - O_CREAT / O_TRUNC / O_APPEND / 可写：目前返回 NotSupported
        //
        // ===== 创建文件（FAT32）应做的详细步骤（实现 LFN 时也适用） =====
        // 目标：在父目录中写入「若干个 LFN 目录项 + 1 个 SFN 目录项」，并返回新文件句柄。
        // 下面以“创建普通文件”为例，目录类似但 attr/size 等字段不同。
        //
        // 0) 路径拆分
        //    - 将 path 拆成 parent_path + file_name。
        //    - 先 open_path(parent_path) 确认父目录存在且 is_dir=true。
        //
        // 1) 遍历父目录簇链，查找可用目录项槽位（32B/entry）
        //    - 父目录的数据就是一个“目录文件”，其内容按 32 字节对齐排列。
        //    - 遍历顺序：
        //        clus = parent_first_clus
        //        while clus != EOC:
        //            读出 clus 对应的簇数据（clus_bytes），每 32B 扫描一个 entry
        //    - 空槽判定：
        //        entry[0] == 0x00  => 后续全空（可用，且可以直接认为目录到此结束）
        //        entry[0] == 0xE5  => 已删除（可复用）
        //    - 若要写 LFN：需要连续 N 个 LFN entry + 1 个 SFN entry 的“连续空槽”。
        //      不足时需要给目录“扩簇”：
        //        alloc_free_cluster() 分配新簇
        //        将目录簇链尾 FAT[tail]=new_clus, FAT[new_clus]=EOC
        //        并把新簇内容清零（这样全是 0x00，等价于一堆空槽）
        //
        // 2) 生成 SFN（短文件名 8.3）与（可选）LFN（长文件名）
        //    - SFN 固定 11 字节：name[0..8] + ext[8..11]，不足用 0x20 空格填充。
        //    - LFN：把 UTF-8 的长名转成 UTF-16，每个 LFN entry 可存 13 个 u16。
        //      多个 LFN entry “倒序”放在 SFN entry 前。
        //      并计算 checksum = lfn_checksum(sfn11)，填入每个 LFN entry 的 LDIR_Chksum。
        //
        // 3) 写入目录项（on-disk 32B 布局/关键字段与偏移）
        //    3.1) LFN entry（attr=0x0F）
        //        - byte 0  : LDIR_Ord（序号；最高位 0x40 表示“最后一段”）
        //        - byte 11 : LDIR_Attr = 0x0F
        //        - byte 12 : LDIR_Type = 0
        //        - byte 13 : LDIR_Chksum = checksum(sfn11)
        //        - byte 26..27 : LDIR_FstClusLO = 0（固定）
        //        - 名字分片（UTF-16，小端）：
        //            Name1: byte 1..10   (5 个 u16)
        //            Name2: byte 14..25  (6 个 u16)
        //            Name3: byte 28..31  (2 个 u16)
        //    3.2) SFN entry（普通 8.3）
        //        - byte 0..10  : DIR_Name[11]（SFN）
        //        - byte 11     : DIR_Attr（普通文件 0x20；目录 0x10）
        //        - byte 20..21 : DIR_FstClusHI（first_cluster 高 16 位，小端）
        //        - byte 26..27 : DIR_FstClusLO（first_cluster 低 16 位，小端）
        //        - byte 28..31 : DIR_FileSize（u32，小端；新建文件为 0）
        //      注意：新建空文件通常 first_cluster=0（表示无簇链），首次写入再分配簇。
        //
        // 4) 目录项落盘
        //    - 将上述 entry 写回父目录对应的 (parent_dir_clus, dirent_offset)
        //      dirent_offset 是“相对父目录文件起点”的字节偏移，必须是 32 的倍数。
        //    - 如果 FAT 有多份副本，后续分配簇/链接簇链时需要同步写所有 FAT 副本。
        //
        // 5) 返回新文件句柄
        //    - 返回时填 first_clus / size / is_dir=false。
        //    - 如果实现写入：write_at 时按 offset 扩簇链，更新 size，并回写 SFN entry 的 size/first_cluster。

        let mut created_loc: Option<(u32, usize)> = None;
        let mut created_sfn11: Option<[u8; 11]> = None;
        let (first_clus, is_dir, size, loc, sfn11) = match self.open_path_with_loc(path) {
            Ok(v) => v,
            Err(VfsFsError::NotFound) if flags.contains(OpenFlags::CREAT) => {
                let (parent_path, file_name) = split_parent(path)?;
                let (pclus, is_dir, _, _, _) = self.open_path_with_loc(&parent_path)?;
                if !is_dir {
                    return Err(VfsFsError::NotDir);
                }
                let (dclus, doff, sfn11) = self.write_name_dirents(pclus, &file_name, 0x20, 0, 0)?;
                created_loc = Some((dclus, doff));
                created_sfn11 = Some(sfn11);
                (0, false, 0, created_loc, created_sfn11)
            }
            Err(e) => return Err(e),
        };

        let init_off = if flags.contains(OpenFlags::APPEND) { size as usize } else { 0 };
        Ok(Arc::new(Fat32File {
            mount_fs,
            first_clus: Mutex::new(first_clus),
            size: Mutex::new(size),
            is_dir,
            offset: Mutex::new(init_off),
            dirent_loc: loc,
            sfn11,
        }))
    }

    fn stat(&mut self, path: &str) -> Result<VfsStat, VfsFsError> {
        if !self.mounted {
            return Err(VfsFsError::Unmounted);
        }
        let (first_clus, is_dir, size) = self.open_path(path)?;
        Ok(VfsStat {
            inode: first_clus,
            size: if is_dir { 0 } else { size as u64 },
            mode: 0,
            file_type: if is_dir { VFS_DT_DIR } else { VFS_DT_REG },
        })
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}