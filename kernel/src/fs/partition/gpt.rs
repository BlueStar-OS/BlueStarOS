//! GPT 分区表解析

use alloc::vec::Vec;
use crate::config::SECTOR_SIZE;

const GPT_SIGNATURE: u64 = 0x5452_4150_2049_4645; // "EFI PART"

#[repr(C, packed)]
struct GptHeader {
    signature: u64,
    revision: u32,
    header_size: u32,
    header_crc32: u32,
    _reserved: u32,
    my_lba: u64,
    alternate_lba: u64,
    first_usable_lba: u64,
    last_usable_lba: u64,
    disk_guid: [u8; 16],
    partition_entry_lba: u64,
    num_partition_entries: u32,
    partition_entry_size: u32,
    partition_entry_crc32: u32,
}

#[repr(C, packed)]
struct GptEntry {
    type_guid: [u8; 16],
    unique_guid: [u8; 16],
    starting_lba: u64,
    ending_lba: u64,
    attributes: u64,
    _name: [u16; 36],
}

pub struct GptPartition {
    pub start_lba: u64,
    pub sectors: u64,
}

fn is_zero_guid(guid: &[u8; 16]) -> bool {
    guid.iter().all(|&b| b == 0)
}

/// 解析 LBA 1 的 GPT 头，返回 (分区条目起始LBA, 条目数, 条目大小)
pub fn parsing_gpt_header(lba1: &[u8; SECTOR_SIZE]) -> Result<(u64, u32, u32), &'static str> {
    let header = unsafe { core::ptr::read_unaligned(lba1.as_ptr() as *const GptHeader) };
    if header.signature != GPT_SIGNATURE {
        return Err("invalid GPT signature");
    }
    Ok((header.partition_entry_lba, header.num_partition_entries, header.partition_entry_size))
}

/// 从分区条目原始数据解析分区列表
pub fn parsing_gpt_entries(data: &[u8], num: u32, entry_size: u32) -> Vec<GptPartition> {
    let mut parts = Vec::new();
    for i in 0..num as usize {
        let off = i * entry_size as usize;
        if off + core::mem::size_of::<GptEntry>() > data.len() {
            break;
        }
        let e = unsafe { core::ptr::read_unaligned(data.as_ptr().add(off) as *const GptEntry) };
        if is_zero_guid(&e.type_guid) || e.ending_lba < e.starting_lba {
            continue;
        }
        parts.push(GptPartition {
            start_lba: e.starting_lba,
            sectors: e.ending_lba - e.starting_lba + 1,
        });
    }
    parts
}
