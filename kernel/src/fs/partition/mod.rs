use crate::fs::partition::gpt::GptPartitionMetadata;
use crate::fs::partition::mbr::MbrPartitionMetadata;

/// 通用分区描述。
///
/// 设计目标：
/// 1. 用统一入口表达 Raw / MBR / GPT 三类分区；
/// 2. 每种分区方案把自己的元数据收敛到对应变体中；
/// 3. 后续按分区标识选择根文件系统时，可以直接读取这里的元数据。
#[derive(Clone, Debug)]
pub enum DevicePartition {
    Raw {
        base_lba: u64,
        sectors: u64,
    },
    Mbr {
        base_lba: u64,
        sectors: u64,
        metadata: MbrPartitionMetadata,
    },
    Gpt {
        base_lba: u64,
        sectors: u64,
        metadata: GptPartitionMetadata,
    },
}

impl DevicePartition {
    pub fn base_lba(&self) -> u64 {
        match self {
            DevicePartition::Raw { base_lba, .. }
            | DevicePartition::Mbr { base_lba, .. }
            | DevicePartition::Gpt { base_lba, .. } => *base_lba,
        }
    }

    pub fn sectors(&self) -> u64 {
        match self {
            DevicePartition::Raw { sectors, .. }
            | DevicePartition::Mbr { sectors, .. }
            | DevicePartition::Gpt { sectors, .. } => *sectors,
        }
    }
}

pub mod gpt;
pub mod mbr;
