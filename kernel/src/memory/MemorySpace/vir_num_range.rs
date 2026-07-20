//! 虚拟页号闭区间 [`VirNumRange`] 及其迭代器。
//!
//! `VirNumRange` 是内存映射子系统里描述“一段连续虚拟页”的基本语义类型：
//! 起止都向下取整到页号，采用闭区间 `[start, end]`，用于代码/数据段映射，
//! 避免因取整方向导致漏映射或多映射。本文件同时提供其迭代器和区间查询工具。

use crate::arch::memory::{VirAddr, VirNumber};
use alloc::vec::Vec;

///开始和结束，一个范围,自动[start,end] start地址自动向下取整，end也向下取整，因为virnumrange用于代码映射，防止代码缺失, startva/PAGE =num+offset ,从num开始，endva/pagesize=endva+offset由于闭区间所以向下取整,防止多映射
#[derive(Debug, Clone, Copy)]
pub struct VirNumRange(pub VirNumber, pub VirNumber);

///VirNumRange迭代器类型
pub struct VirNumRangeIter {
    current: VirNumber,
    end: VirNumber,
}

impl Iterator for VirNumRangeIter {
    type Item = VirNumber;
    fn next(&mut self) -> Option<Self::Item> {
        let current = self.current;
        let end = self.end;
        if current.0 <= end.0 {
            let cur = current.0;
            self.current.step();
            Some(VirNumber(cur))
        } else {
            None
        }
    }
}

impl IntoIterator for VirNumRange {
    type IntoIter = VirNumRangeIter;
    type Item = VirNumber;
    fn into_iter(self) -> Self::IntoIter {
        VirNumRangeIter {
            current: self.0,
            end: self.1,
        }
    }
}

impl VirNumRange {
    ///左端点
    pub fn left_point(&self) -> VirNumber {
        self.0
    }

    ///右端点
    pub fn right_point(&self) -> VirNumber {
        self.1
    }

    ///VirNumRange初始化 传入起始地址和结束地址,闭区间都需要映射 [start,end] start地址自动向下取整，end也向下取整
    pub fn new(start: VirAddr, end: VirAddr) -> Self {
        let start_vpn = start.floor_down();
        let end_vpn = end.floor_down();
        VirNumRange(start_vpn, end_vpn) //闭区间，都需要映射
    }
    ///查找区间是否包含某个vpn号 自身是闭区间
    pub fn is_contain_thisvpn(&self, vpn: VirNumber) -> bool {
        let start = self.0;
        let end = self.1;
        //闭区间
        vpn >= start && vpn <= end
    }

    ///查找区间是和这个区间有交集 自身是闭区间
    pub fn is_contain_thisvpn_range(&self, vpn_range: VirNumRange) -> Vec<VirNumber> {
        let start = self.0;
        let end = self.1;
        let target_start = vpn_range.0;
        let target_end = vpn_range.1;

        let inter_start = if start >= target_start {
            start
        } else {
            target_start
        };
        let inter_end = if end <= target_end { end } else { target_end };

        if inter_start > inter_end {
            return Vec::new();
        }

        let mut result = Vec::new();
        for vpn in VirNumRange(inter_start, inter_end) {
            result.push(vpn);
        }
        result
    }
}
