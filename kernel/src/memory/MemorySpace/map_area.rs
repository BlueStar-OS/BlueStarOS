//! 单个映射区域 [`MapArea`]：一段连续虚拟页 + 其页帧与映射策略。
//!
//! `MapArea` 是地址空间的最小管理单元，持有一段 `VirNumRange`、访问权限
//! `MapAreaFlags`、映射方式 `MapType`（恒等/普通），以及本区域实际占有的
//! 物理页帧表和可选的 mmap 记账信息。本文件实现区域内的建/拆映射（`map_one`
//! /`map_all`/`unmap_one`）与 ELF 段数据拷贝（`copy_data`）。

use super::flags::{MapAreaFlags, MapType};
use super::mmap_entry::MmapEntry;
use super::vir_num_range::VirNumRange;
use crate::arch::memory::{PageTable, PhysiNumber, VirNumber};
use crate::config::PAGE_SIZE;
use crate::memory::{alloc_frame, frame_allocator::FramTracker};
use crate::IRPG_OFFSET;
use alloc::collections::btree_map::BTreeMap;
use alloc::sync::Arc;
use log::{error, trace};

#[derive(Clone)]
pub struct MapArea {
    //通常为单次push进来，虽然粒度大，保证push粒度足够小即可
    ///虚拟页号范围,闭区间
    pub range: VirNumRange,
    pub(crate) flags: MapAreaFlags, //访问标志
    pub frames: BTreeMap<VirNumber, Arc<FramTracker>>, //Maparea 持有的物理页
    pub(crate) map_type: MapType,
    /// 非空 => 该 area 由 mmap 创建，按页记录懒分配/文件映射信息。
    pub mmap: MmapEntry,
}

impl MapArea {
    ///range,闭区间
    pub fn new(range: VirNumRange, flags: MapAreaFlags, map_type: MapType) -> Self {
        MapArea {
            range,
            flags,
            frames: BTreeMap::new(),
            map_type,
            mmap: MmapEntry::new(),
        }
    }

    /// `first_vpn_ppn` 补丁： 是否已经映射过这个vpn，如果有，请把那个vpn对应的ppnclone一份
    /// 更新 MapArea 的访问权限标志（用于 mprotect）
    pub fn set_flags(&mut self, flags: MapAreaFlags) {
        self.flags = flags;
    }

    pub fn map_one(
        &mut self,
        vpn: VirNumber,
        page_table: &mut PageTable,
        first_vpn_ppn: Option<Arc<FramTracker>>,
    ) {
        //带自动分配物理页帧的
        //可能是恒等和普通映射
        let ppn: PhysiNumber;
        let is_maped = page_table.is_maped(vpn); // 如果这个vpn已经映射并且合法，应该让它指向相同的ppn并且权限合并
        match self.map_type {
            MapType::Indentical => {
                // trace!("Identical map");
                ppn = PhysiNumber(vpn.0) //内核特权高大上，恒等映射 内核映射所有物理帧，但是不能占用和分配对应Framtracer，需要构建一个特殊页表
            }
            MapType::Maped => {
                if !is_maped {
                    let frame = alloc_frame().expect("Memory Alloc Failed By map_one");
                    ppn = frame.ppn;
                    self.frames.insert(vpn, Arc::new(frame)); //管理最终pte对应的frametracer，分工明确 巧妙！！！！
                    trace!("map vpn:{}->ppn:{}", vpn.0, ppn.0)
                } else {
                    let last = first_vpn_ppn.expect("Please give last vpn's ppn");
                    // 寻找已经映射的vpn的那个ppn是多少，不能重复分配，但是执行同一个ppn号，也要防止doublefree.这里Arc可以保证
                    ppn = last.ppn;
                    self.frames.insert(vpn, last);
                }
            }
        };
        // 权限合并
        page_table.map(vpn, ppn, self.flags.into());
        //debug!("Map Aread map vpn:{} -> ppn:{}",vpn.0,ppn.0);
    }

    pub fn map_one_with_frame(
        &mut self,
        vpn: VirNumber,
        frame: Arc<FramTracker>,
        page_table: &mut PageTable,
    ) {
        if page_table.is_maped(vpn) {
            return;
        }
        let ppn = frame.ppn;
        self.frames.insert(vpn, frame);
        page_table.map(vpn, ppn, self.flags.into());
    }

    ///映射分割和挂载MapArea所有段,闭区间全部映射
    pub fn map_all(&mut self, page_table: &mut PageTable, first_vpn_ppn: Option<Arc<FramTracker>>) {
        let start = self.range.0;
        let end = self.range.1;
        let mut current = start;
        while current.0 <= end.0 {
            self.map_one(current, page_table, first_vpn_ppn.clone());
            current.0 += 1;
        }
    }

    ///通过虚拟页号释放一个页帧
    pub fn unmap_one(&mut self, table: &mut PageTable, vpn: VirNumber) {
        if self.frames.contains_key(&vpn) {
            self.frames
                .remove(&vpn.clone())
                .expect("Remove a exist vpn failed!!"); //回收页帧(Arc drop 时真正释放)
            table.unmap(vpn);
        } else {
            error!(
                "MapArea try Unmap vpn:{} but not find vpn in this area",
                vpn.0
            );
        }
    }

    ///复制MAPED映射的数据到物理页帧,maped方式才调用它(不包含判断)  必须按照elf格式的顺序复制,传入的data需要自行截断，有栈等映射不需要复制数据
    pub fn copy_data(&mut self, data: Option<(IRPG_OFFSET, &[u8])>, table: &mut PageTable) {
        if data.is_none() {
            return;
        }

        // 解构出：页内偏移量(如0x40) 和 源数据切片
        let (mut page_offset, src_data) = data.unwrap();

        // 先把range全部清0 bss等
        self.range.into_iter().for_each(|vpn| {
            table.get_mut_byte(vpn).expect("Cant get mut slice").fill(0);
        });

        let mut current_vpn = self.range.0;
        let mut current_src_idx = 0; // 记录源数据已经拷贝了多少字节
        let total_len = src_data.len();

        loop {
            // 1. 计算这一页还剩多少空间可以写 (4096 - offset)
            let available_in_page = PAGE_SIZE - page_offset.raw();

            // 2. 计算还剩多少源数据没拷
            let remaining_src = total_len - current_src_idx;

            // 3. 决定本次拷贝的长度：取最小值
            let copy_len = available_in_page.min(remaining_src);

            // 如果没数据可拷了，退出
            if copy_len == 0 {
                break;
            }

            // 4. 获取目标物理页（整个4096字节）
            let dst_page = table.get_mut_byte(current_vpn).expect("Cant get mut slice");

            // 5. 【关键】源数据：从 current_src_idx 往后取 copy_len 个
            let src = &src_data[current_src_idx..current_src_idx + copy_len];

            // 6. 【关键】目标数据：从 page_offset 往后写 copy_len 个
            let dst = &mut dst_page[page_offset.raw()..page_offset.raw() + copy_len];

            // 执行拷贝
            dst.copy_from_slice(src);

            // 更新游标
            current_src_idx += copy_len;
            current_vpn.step();

            // 重点！除了第一页可能有偏移量，后续所有页都必须从 0 开始写
            page_offset = IRPG_OFFSET::new(0);
        }
    }
}
