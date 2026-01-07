//! Fat32 file system
//!
//! Endianness: FAT32 on-disk fields are little-endian.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::any::Any;
use alloc::vec;
use spin::Mutex;

use crate::fs::vfs::{File, LinuxDirent64, MountFs, OpenFlags, VfsFs, VfsFsError, VfsStat, VFS_DT_DIR, VFS_DT_REG};

fn le16(b: &[u8]) -> u16 {
    u16::from_le_bytes([b[0], b[1]])
}

fn le32(b: &[u8]) -> u32 {
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

pub struct Fat32Fs{
    pub dev: Arc<dyn File>, // vblock 分区设备（按“分区内偏移”读写：offset=0 表示分区第0字节）
    pub info: Fat32Info,     // 从 BPB/FSInfo 推导出来的几何与布局信息
    mounted: bool,
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

fn parse_lfn_part(e: &[u8]) -> Vec<u16> {
    let mut out: Vec<u16> = Vec::new();
    let mut push_u16 = |lo: u8, hi: u8| {
        out.push(u16::from_le_bytes([lo, hi]));
    };
    // name1: 1..10 (5 UTF-16)
    for i in (1..11).step_by(2) {
        push_u16(e[i], e[i + 1]);
    }
    // name2: 14..25 (6 UTF-16)
    for i in (14..26).step_by(2) {
        push_u16(e[i], e[i + 1]);
    }
    // name3: 28..31 (2 UTF-16)
    for i in (28..32).step_by(2) {
        push_u16(e[i], e[i + 1]);
    }
    out
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
        let v = le32(&buf[ent_off..ent_off + 4]) & 0x0FFFFFFF;
        Ok(v)
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

    fn walk_dir_find(&self, dir_first: u32, target: &str) -> Result<DirEnt, VfsFsError> {
        let mut clus = dir_first;
        let mut buf = vec![0u8; self.info.clus_bytes as usize];
        let mut lfn_parts: Vec<(u8, Vec<u16>)> = Vec::new();
        let mut lfn_ck: Option<u8> = None;

        loop {
            self.read_cluster(clus, &mut buf)?;
            for off in (0..buf.len()).step_by(DIR_ENTRY_SIZE) {
                let e = &buf[off..off + DIR_ENTRY_SIZE];
                let first = e[0];
                if first == 0x00 {
                    return Err(VfsFsError::NotFound);
                }
                if first == 0xE5 {
                    lfn_parts.clear();
                    lfn_ck = None;
                    continue;
                }
                let attr = e[11];
                if attr == ATTR_LONG_NAME {
                    let ord = e[0];
                    let seq = ord & 0x1F;
                    let ck = e[13];
                    if (ord & 0x40) != 0 {
                        lfn_parts.clear();
                        lfn_ck = Some(ck);
                    }
                    if lfn_ck.is_some() && lfn_ck == Some(ck) {
                        lfn_parts.push((seq, parse_lfn_part(e)));
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

                let mut name11 = [0u8; 11];
                name11.copy_from_slice(&e[0..11]);
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
                    let hi = le16(&e[20..22]) as u32;
                    let lo = le16(&e[26..28]) as u32;
                    let first_clus = (hi << 16) | lo;
                    let size = le32(&e[28..32]);
                    return Ok(DirEnt { attr, first_clus, size });
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

    fn dir_getdents(&self, dir_first: u32, max_len: usize) -> Result<Vec<u8>, VfsFsError> {
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
                let first = e[0];
                if first == 0x00 {
                    return Ok(stream);
                }
                if first == 0xE5 {
                    lfn_parts.clear();
                    lfn_ck = None;
                    continue;
                }
                let attr = e[11];
                if attr == ATTR_LONG_NAME {
                    let ord = e[0];
                    let seq = ord & 0x1F;
                    let ck = e[13];
                    if (ord & 0x40) != 0 {
                        lfn_parts.clear();
                        lfn_ck = Some(ck);
                    }
                    if lfn_ck.is_some() && lfn_ck == Some(ck) {
                        lfn_parts.push((seq, parse_lfn_part(e)));
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

                let mut name11 = [0u8; 11];
                name11.copy_from_slice(&e[0..11]);
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

                let hi = le16(&e[20..22]) as u32;
                let lo = le16(&e[26..28]) as u32;
                let first_clus = (hi << 16) | lo;
                let dtype = if (attr & ATTR_DIRECTORY) != 0 { VFS_DT_DIR } else { VFS_DT_REG };

                let name_bytes = name.as_bytes();
                let reclen = align_up(hdr_len + name_bytes.len() + 1, 8);
                if stream.len() + reclen > max_len {
                    return Ok(stream);
                }
                let base = stream.len();
                stream.resize(base + reclen, 0);
                cur_off = cur_off.saturating_add(reclen as u64);

                let hdr = LinuxDirent64 {
                    d_ino: first_clus as u64,
                    d_off: cur_off,
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
    first_clus: u32,
    size: u32,
    is_dir: bool,
    offset: Mutex<usize>,
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

    fn write(&self, _buf: &[u8]) -> Result<usize, VfsFsError> {
        Err(VfsFsError::NotSupported)
    }

    fn read_at(&self, offset: usize, buf: &mut [u8]) -> Result<usize, VfsFsError> {
        if self.is_dir {
            return Err(VfsFsError::IsDir);
        }
        self.with_fs(|fs| fs.read_file_at(self.first_clus, self.size, offset, buf))
    }

    fn write_at(&self, _offset: usize, _buf: &[u8]) -> Result<usize, VfsFsError> {
        Err(VfsFsError::NotSupported)
    }

    fn lseek(&self, offset: isize, whence: usize) -> Result<usize, VfsFsError> {
        let mut off = self.offset.lock();
        let cur = *off as isize;
        let end = if self.is_dir { 0 } else { self.size as isize };
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
        self.with_fs(|fs| fs.dir_getdents(self.first_clus, max_len))
    }

    fn stat(&self) -> Result<VfsStat, VfsFsError> {
        Ok(VfsStat {
            inode: self.first_clus,
            size: if self.is_dir { 0 } else { self.size as u64 },
            mode: 0,
            file_type: if self.is_dir { VFS_DT_DIR } else { VFS_DT_REG },
        })
    }
}

impl VfsFs for Fat32Fs {
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

    fn open(&mut self, mount_fs: MountFs, path: &str, flags: OpenFlags) -> Result<Arc<dyn File>, VfsFsError> {
        if !self.mounted {
            return Err(VfsFsError::Unmounted);
        }
        if flags.writable()
            || flags.contains(OpenFlags::APPEND)
            || flags.contains(OpenFlags::CREAT)
            || flags.contains(OpenFlags::TRUNC)
        {
            return Err(VfsFsError::NotSupported);
        }
        let (first_clus, is_dir, size) = self.open_path(path)?;
        Ok(Arc::new(Fat32File {
            mount_fs,
            first_clus,
            size,
            is_dir,
            offset: Mutex::new(0),
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