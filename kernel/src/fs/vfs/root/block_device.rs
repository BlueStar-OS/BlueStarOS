use super::{RootFs, ROOTFS};
use crate::config::SECTOR_SIZE;
use crate::fs::fs_backend::RamFs;
use crate::fs::partition::gpt::{parsing_gpt_entries, parsing_gpt_header, GptPartitionName};
use crate::fs::partition::mbr::{parsing_mbr_partition, MbrPartitionType};
use crate::fs::vfs::dm_liner::DmlinerEntry;
use crate::fs::vfs::{global_block_devices, BlockDevTrait, File, VfsFsError, BLOCKDEVFILE};
use alloc::{string::String, sync::Arc, vec::Vec};
use log::{debug, warn};
use spin::Mutex;

/// 文件系统探测候选。
///
/// 这里封装一个“可直接尝试挂载的块设备入口”，
/// 并附带最少量的分区元数据，供根文件系统选择策略使用。
pub struct FilesystemProbeCandidate {
    pub path: String,
    pub device: Arc<dyn crate::fs::vfs::File>,
    pub is_gpt_rootfs: bool,
}

impl RootFs {
    /// 将块设备索引转换为 `/vda`、`/vdb`、`/vdc` 这样的设备名前缀。
    ///
    /// 这里使用类似 Excel 列名的方式做 26 进制展开：
    /// 0 -> a, 1 -> b, ..., 25 -> z, 26 -> aa
    fn block_device_name(index: usize) -> String {
        let mut n = index;
        let mut suffix = String::new();

        loop {
            let ch = (b'a' + (n % 26) as u8) as char;
            suffix.insert(0, ch);
            if n < 26 {
                break;
            }
            n = n / 26 - 1;
        }

        alloc::format!("/vd{}", suffix)
    }

    /// 为块设备整盘设备文件，并且尝试扫描这个盘的分区来建立分区设备文件节点。
    ///
    /// 行为约定：
    /// 1. 总是先创建原始块设备节点，例如 `/vda`；
    /// 2. 然后尽力解析 MBR；
    /// 3. 若 MBR 不存在或无效，再尝试 GPT；
    /// 4. 分区表解析失败不会中断整个块设备扫描流程，只会保留原始节点。
    fn register_block_device_nodes(
        ramfs: &mut RamFs,
        dev_path: &str,
        blk: Arc<Mutex<dyn BlockDevTrait>>,
    ) -> Result<(), VfsFsError> {
        let total_sectors = blk.lock().capacity_in_sectors();
        let whole = Arc::new(BLOCKDEVFILE::new(
            blk.clone(),
            DmlinerEntry::new(0, total_sectors),
        )) as Arc<dyn crate::fs::vfs::File>;
        ramfs.mkdev(dev_path, whole)?;

        let mut mbr: [u8; SECTOR_SIZE] = [0; SECTOR_SIZE];
        if let Err(err) = blk.lock().read_block(0, &mut mbr) {
            warn!("Read {}  MBR failed: {:?}", dev_path, err);
            return Ok(());
        }

        let parts = parsing_mbr_partition(mbr);
        let mbr_ok = parts.as_ref().is_ok_and(|v| !v.is_empty());
        let is_protective_mbr = parts.as_ref().is_ok_and(|entries| {
            !entries.is_empty()
                && entries.iter().all(|entry| {
                    matches!(
                        entry.metadata.partition_type,
                        MbrPartitionType::ProtectiveGpt
                    )
                })
        });
        debug!(
            "{} MBR parse result: ok={}, protective={}, count={}",
            dev_path,
            mbr_ok,
            is_protective_mbr,
            parts.as_ref().map_or(0, |v| v.len())
        );

        if mbr_ok && !is_protective_mbr {
            for (idx, entry) in parts.unwrap().into_iter().enumerate() {
                debug!(
                    "{} MBR partition {}: start_lba={}, sectors={}, meta={:?}",
                    dev_path, idx, entry.start_lba, entry.sectors, entry.metadata
                );
                let dev = Arc::new(BLOCKDEVFILE::new(
                    blk.clone(),
                    DmlinerEntry::new(entry.start_lba, entry.sectors),
                )) as Arc<dyn crate::fs::vfs::File>;
                let path = alloc::format!("{}{}", dev_path, idx + 1);
                ramfs.mkdev(path.as_str(), dev)?;
            }
            return Ok(());
        }

        debug!("{} Try Parse GPT Partition table", dev_path);
        let mut lba1: [u8; SECTOR_SIZE] = [0; SECTOR_SIZE];
        if let Err(err) = blk.lock().read_block(1, &mut lba1) {
            warn!("读取 {} 的 GPT Header 失败: {:?}", dev_path, err);
            return Ok(());
        }

        let (entry_lba, num_entries, entry_size) = match parsing_gpt_header(&lba1) {
            Ok(v) => v,
            Err(e) => {
                warn!("{} GPT Header parse failed: {}", dev_path, e);
                return Ok(());
            }
        };
        debug!(
            "{} GPT header: entry_lba={}, num_entries={}, entry_size={}",
            dev_path, entry_lba, num_entries, entry_size
        );

        let entries_bytes = num_entries as usize * entry_size as usize;
        let entry_sectors = entries_bytes.div_ceil(SECTOR_SIZE);
        let mut entry_buf = alloc::vec![0u8; entry_sectors * SECTOR_SIZE];
        for s in 0..entry_sectors {
            if let Err(err) = blk.lock().read_block(
                entry_lba as usize + s,
                &mut entry_buf[s * SECTOR_SIZE..(s + 1) * SECTOR_SIZE],
            ) {
                warn!("读取 {} 的 GPT 分区项失败: {:?}", dev_path, err);
                return Ok(());
            }
        }

        let gpt_parts = parsing_gpt_entries(&entry_buf, num_entries, entry_size);
        debug!("{} GPT found {} partitions", dev_path, gpt_parts.len());
        for (idx, gp) in gpt_parts.into_iter().enumerate() {
            debug!(
                "{} GPT partition {}: start_lba={}, sectors={}, meta={:?}",
                dev_path, idx, gp.start_lba, gp.sectors, gp.metadata
            );
            let dev = Arc::new(BLOCKDEVFILE::new(
                blk.clone(),
                DmlinerEntry::new(gp.start_lba, gp.sectors),
            )) as Arc<dyn crate::fs::vfs::File>;
            let path = alloc::format!("{}{}", dev_path, idx + 1);
            ramfs.mkdev(path.as_str(), dev)?;
        }

        Ok(())
    }

    /// 从全局块设备表里收集文件系统探测候选。
    ///
    /// 顺序是：
    /// 1. 整盘 Raw；
    /// 2. MBR 分区；
    /// 3. GPT 分区。
    ///
    /// 注意：
    /// 这里收集的是“块设备访问入口”，并不绑定具体文件系统类型。
    /// 调用方既可以拿这些候选去探测 ext4，也可以探测 FAT32 或其他文件系统。
    pub fn collect_filesystem_probe_candidates() -> Vec<FilesystemProbeCandidate> {
        let mut filesystem_candidates = alloc::vec::Vec::new();

        for (idx, blk) in global_block_devices().into_iter().enumerate() {
            let dev_path = Self::block_device_name(idx);
            let total_sectors = blk.lock().capacity_in_sectors();

            let raw = Arc::new(BLOCKDEVFILE::new(
                blk.clone(),
                DmlinerEntry::new(0, total_sectors),
            )) as Arc<dyn File>;
            filesystem_candidates.push(FilesystemProbeCandidate {
                path: dev_path.clone(),
                device: raw,
                is_gpt_rootfs: false,
            });

            let mut mbr: [u8; SECTOR_SIZE] = [0; SECTOR_SIZE];
            if blk.lock().read_block(0, &mut mbr).is_err() {
                continue;
            }

            if let Ok(parts) = parsing_mbr_partition(mbr) {
                let is_protective_mbr = !parts.is_empty()
                    && parts.iter().all(|entry| {
                        matches!(
                            entry.metadata.partition_type,
                            MbrPartitionType::ProtectiveGpt
                        )
                    });
                if is_protective_mbr {
                    debug!("{} has protective MBR, continue parse GPT", dev_path);
                } else {
                    for (part_idx, entry) in parts.into_iter().enumerate() {
                        let part_path = alloc::format!("{}{}", dev_path, part_idx + 1);
                        let part = Arc::new(BLOCKDEVFILE::new(
                            blk.clone(),
                            DmlinerEntry::new(entry.start_lba, entry.sectors),
                        )) as Arc<dyn File>;
                        filesystem_candidates.push(FilesystemProbeCandidate {
                            path: part_path,
                            device: part,
                            is_gpt_rootfs: false,
                        });
                    }
                    continue;
                }
            }

            let mut lba1: [u8; SECTOR_SIZE] = [0; SECTOR_SIZE];
            if blk.lock().read_block(1, &mut lba1).is_err() {
                continue;
            }

            let Ok((entry_lba, num_entries, entry_size)) = parsing_gpt_header(&lba1) else {
                continue;
            };

            let entries_bytes = num_entries as usize * entry_size as usize;
            let entry_sectors = entries_bytes.div_ceil(SECTOR_SIZE);
            let mut entry_buf = alloc::vec![0u8; entry_sectors * SECTOR_SIZE];
            let mut read_ok = true;
            for sector_idx in 0..entry_sectors {
                if blk
                    .lock()
                    .read_block(
                        entry_lba as usize + sector_idx,
                        &mut entry_buf[sector_idx * SECTOR_SIZE..(sector_idx + 1) * SECTOR_SIZE],
                    )
                    .is_err()
                {
                    read_ok = false;
                    break;
                }
            }
            if !read_ok {
                continue;
            }

            for (part_idx, entry) in parsing_gpt_entries(&entry_buf, num_entries, entry_size)
                .into_iter()
                .enumerate()
            {
                let part_path = alloc::format!("{}{}", dev_path, part_idx + 1);
                let is_gpt_rootfs = matches!(&entry.metadata.name, GptPartitionName::Named(name) if name == "rootfs");
                let part = Arc::new(BLOCKDEVFILE::new(
                    blk.clone(),
                    DmlinerEntry::new(entry.start_lba, entry.sectors),
                )) as Arc<dyn File>;
                filesystem_candidates.push(FilesystemProbeCandidate {
                    path: part_path,
                    device: part,
                    is_gpt_rootfs,
                });
            }
        }

        filesystem_candidates
    }

    /// 扫描全部已注册块设备，并在根 ramfs 下创建块设备节点。
    pub fn scan_and_build_vblock_device() -> Result<(), VfsFsError> {
        let root = ROOTFS.lock();
        let root = root.as_ref().ok_or(VfsFsError::IO)?;
        let (fs, sub) = root.resolve_mount_point("/")?.ok_or(VfsFsError::NotFound)?;
        if sub != "/" {
            return Err(VfsFsError::IO);
        }

        let mut guard = fs.lock();
        let ramfs = guard
            .as_any_mut()
            .downcast_mut::<RamFs>()
            .ok_or(VfsFsError::NotSupported)?;

        let blocks = global_block_devices();
        if blocks.is_empty() {
            return Err(VfsFsError::IO);
        }

        warn!("find {} block device", blocks.len());

        for (idx, blk) in blocks.into_iter().enumerate() {
            let dev_path = Self::block_device_name(idx);
            warn!("Registe block {}", dev_path);
            if let Err(err) = Self::register_block_device_nodes(ramfs, dev_path.as_str(), blk) {
                warn!("Register Block device {} failed: {:?}", dev_path, err);
            }
        }

        Ok(())
    }
}
