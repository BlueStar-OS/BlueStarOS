use crate::fs::partition::mbr::mbr_entry;

pub enum DevicePartition {
    MBR(mbr_entry),
    Raw { base_lba: u64, sectors: u64 },
}

impl DevicePartition {
    pub fn base_lba(&self) -> u64 {
        match self {
            DevicePartition::MBR(e) => e.start_lbn as u64,
            DevicePartition::Raw { base_lba, .. } => *base_lba,
        }
    }

    pub fn sectors(&self) -> u64 {
        match self {
            DevicePartition::MBR(e) => e.len as u64,
            DevicePartition::Raw { sectors, .. } => *sectors,
        }
    }
}



pub mod mbr;
