//!MBR分区表解析 little-endian

use alloc::vec::Vec;
use crate::config::SECTOR_SIZE;
const MBR_OFFSET:usize = 0x1BE; //MBR表开始字节 byte
const PER_ENTRY:usize = 16; //byte
enum BootIndicator {
    Active =0x80,
    UnActive = 0x00
}
impl TryFrom<u8> for BootIndicator {
    type Error = &'static str;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value{
            0x80=>{Ok(BootIndicator::Active)},
            0x00=>{Ok(BootIndicator::UnActive)},
            _=>{Err("invalid bootindicator")}
        }
    }
}

pub enum FsType {
    Linux,
    Fat16,
    Fat32,
}

pub struct mbr_entry{
    pub partiton_type: FsType,
    pub start_lbn:u32,
    pub len:u32,
    pub bootable: bool,
}

///解析mbr分区表
pub fn parsing_mbr_partition(data:[u8;SECTOR_SIZE]) -> Result<Vec<mbr_entry>, &'static str>{
    if data[510] != 0x55 || data[511] != 0xAA{
        return Err("invalid mbr signature");
    }

    let mut partitions: Vec<mbr_entry> = Vec::new();
    for i in 0..4usize {
        let base = MBR_OFFSET + i * PER_ENTRY;
        let boot = BootIndicator::try_from(data[base])?;
        //1..3 为chs start
        let partition_type = data[base + 4];
        //0x00 = unused
        //0x83 = Linux（ext2/3/4 常见）
        //0x0e = FAT16 LBA
        //0x0b/0x0c = FAT32（0c 是 LBA 方式）
        //5..7 chs end
        let start_lbn = u32::from_le_bytes(
            data[base + 8..base + 12]
                .try_into()
                .map_err(|_| "invalid mbr entry")?,
        );
        let block_count = u32::from_le_bytes(
            data[base + 12..base + 16]
                .try_into()
                .map_err(|_| "invalid mbr entry")?,
        );

        if partition_type == 0x00 || block_count == 0 {
            continue;
        }

        let fs_type = match partition_type {
            0x83 => FsType::Linux, // TODO：简化，实际需要在Mount探测文件系统类型
            0x0e => FsType::Fat16,
            0x0b | 0x0c => FsType::Fat32,
            _ => {
                continue;
            }
        };

        partitions.push(mbr_entry {
            partiton_type: fs_type,
            start_lbn,
            len: block_count,
            bootable: matches!(boot, BootIndicator::Active),
        });
    }

    Ok(partitions)
}