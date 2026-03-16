//!MBR分区表解析 little-endian
//！ 不完整支持 MBR 的所有逻辑分区。不支持扩展分区
use crate::config::SECTOR_SIZE;
use alloc::vec::Vec;

const MBR_OFFSET: usize = 0x1BE; //MBR表开始字节 byte
const PER_ENTRY: usize = 16; //byte
const MAX_MBR_PARTITION: usize = 4; //MBR最多4个主分区
enum BootIndicator {
    Active = 0x80,
    UnActive = 0x00,
}

impl TryFrom<u8> for BootIndicator {
    type Error = &'static str;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x80 => Ok(BootIndicator::Active),
            0x00 => Ok(BootIndicator::UnActive),
            _ => Err("invalid bootindicator"),
        }
    }
}

/// MBR 分区类型语义。
///
/// 保留原始 `type_code`，同时给出内核目前能理解的常见语义分类。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MbrPartitionType {
    Linux,
    Fat16,
    Fat32,
    Extended,
    ProtectiveGpt,
    Unknown(u8),
}

/// MBR 分区元数据。
#[derive(Clone, Debug)]
pub struct MbrPartitionMetadata {
    pub type_code: u8,
    pub bootable: bool,
    pub is_extended: bool,
    pub partition_type: MbrPartitionType,
}

/// 单个 MBR 分区条目。
#[derive(Clone, Debug)]
pub struct MbrPartitionEntry {
    pub start_lba: u64,
    pub sectors: u64,
    pub metadata: MbrPartitionMetadata,
}

///解析mbr分区表
pub fn parsing_mbr_partition(
    data: [u8; SECTOR_SIZE],
) -> Result<Vec<MbrPartitionEntry>, &'static str> {
    if data[510] != 0x55 || data[511] != 0xAA {
        return Err("invalid mbr signature");
    }

    let mut partitions: Vec<MbrPartitionEntry> = Vec::new();
    for i in 0..MAX_MBR_PARTITION {
        let base = MBR_OFFSET + i * PER_ENTRY;
        let boot = BootIndicator::try_from(data[base])?;
        //1..3 为chs start
        let type_code = data[base + 4];
        //0x00 = unused
        //0x83 = Linux（ext2/3/4 常见）
        //0x0e = FAT16 LBA
        //0x0b/0x0c = FAT32（0c 是 LBA 方式）
        //5..7 chs end
        let start_lba = u32::from_le_bytes(
            data[base + 8..base + 12]
                .try_into()
                .map_err(|_| "invalid mbr entry")?,
        );
        let sectors = u32::from_le_bytes(
            data[base + 12..base + 16]
                .try_into()
                .map_err(|_| "invalid mbr entry")?,
        );

        if type_code == 0x00 || sectors == 0 {
            continue;
        }

        let partition_type = match type_code {
            0x83 => MbrPartitionType::Linux,
            0x0e | 0x06 => MbrPartitionType::Fat16,
            0x0b | 0x0c => MbrPartitionType::Fat32,
            0x05 | 0x0f | 0x85 => MbrPartitionType::Extended,
            0xee => MbrPartitionType::ProtectiveGpt,
            other => MbrPartitionType::Unknown(other),
        };
        let is_extended = matches!(partition_type, MbrPartitionType::Extended);

        partitions.push(MbrPartitionEntry {
            start_lba: start_lba as u64,
            sectors: sectors as u64,
            metadata: MbrPartitionMetadata {
                type_code,
                bootable: matches!(boot, BootIndicator::Active),
                is_extended,
                partition_type,
            },
        });
    }

    Ok(partitions)
}
