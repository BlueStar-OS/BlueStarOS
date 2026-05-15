//! e1000 发送描述符环形队列
//!
//! 参考 Linux: drivers/net/ethernet/intel/e1000/e1000_main.c
//!
//! ## 数据结构 (Linux 对齐)
//!
//! | 类型 | 对应 Linux | 文件:行号 |
//! |------|------------|-----------|
//! | `E1000TxDesc` | `struct e1000_tx_desc` | e1000_hw.h:506 |
//! | `E1000TxBuffer` | `struct e1000_tx_buffer` | e1000.h:136 |
//! | `E1000TxRing` | `struct e1000_tx_ring` | e1000.h:165 |
//!
//! ## 操作函数 (Linux 风格)
//!
//! | 函数 | 对应 Linux | 功能 |
//! |------|------------|------|
//! | `e1000_setup_tx_resources()` | `e1000_setup_tx_resources()` | 分配 TX desc ring DMA 内存 + buffer_info |
//! | `e1000_configure_tx()` | `e1000_configure_tx()` | 写入 TDBAL/TDLEN/TCTL, 使能发送 |
//! | `e1000_clean_tx_irq()` | `e1000_clean_tx_irq()` | TX 完成回收 (DD=1 释放已发送缓冲区) |
//! | `e1000_transmit()` | `e1000_xmit_frame()` 简化 | 提交一帧到 TX ring |
//!
//! ## 数据流
//! ```text
//! probe:
//!   e1000_setup_tx_resources()  — 分配 desc ring + buffer_info
//!   e1000_configure_tx()         — 写入 TDBAL/TDLEN/TCTL, 使能发送
//!
//! send:
//!   e1000_transmit(data)         — 写 desc + wmb + 写 TDT doorbell
//!   e1000_clean_tx_irq()         — DD=1 回收已发送 buffer (中断或轮询调用)
//! ```

use alloc::vec::Vec;
use log::{debug, info, warn};

use crate::config::PAGE_SIZE;
use crate::driver::network::e1000::netbuffer::NetBuffer;
use crate::driver::pcie::BarSpace;
use crate::memory::{alloc_contiguous_frames, alloc_frame, FramTracker};

// ============================================================================
// DMA 内存屏障 — RISC-V Weak Memory Order
// ============================================================================

/// DMA 写屏障: 保证所有设备内存写操作在后续写之前完成
///
/// 在写入描述符字段之后、写入 TDT doorbell 寄存器之前必须调用。
/// 确保硬件在收到 TDT 更新时看到的 descriptor/buffer_addr 是最新值。
///
/// 参考: e1000_main.c:4656 — `dma_wmb()`, Linux
///       arch/riscv/include/asm/barrier.h (RISC-V: RISCV_FENCE(iow, iow))
#[inline(always)]
fn dma_wmb() {
    // SAFETY: fence ow,ow 是 RISC-V 标准指令, 纯 CPU 排序, 无副作用。
    // OW = 设备输出 + 普通写; 屏障强制所有先前写在后续写之前可见。
    unsafe {
        core::arch::asm!("fence ow, ow", options(nostack, preserves_flags));
    }
}

// ============================================================================
// 寄存器偏移量 (参考 Linux e1000_hw.h)
// ============================================================================

/// TX Descriptor Base Address Low   —  e1000_hw.h:888
const E1000_TDBAL: usize = 0x03800;
/// TX Descriptor Base Address High  —  e1000_hw.h:889
const E1000_TDBAH: usize = 0x03804;
/// TX Descriptor Length             —  e1000_hw.h:890
const E1000_TDLEN: usize = 0x03808;
/// TX Descriptor Head               —  e1000_hw.h:891
const E1000_TDH: usize = 0x03810;
/// TX Descriptor Tail               —  e1000_hw.h:892
const E1000_TDT: usize = 0x03818;

/// TX Control                       —  e1000_hw.h:846
const E1000_TCTL: usize = 0x00400;
/// TX Inter-packet Gap              —  e1000_hw.h:854
const E1000_TIPG: usize = 0x00410;

// ============================================================================
// TCTL 位定义 (参考 Linux e1000_hw.h:1861-1888)
// ============================================================================

const E1000_TCTL_EN: u32 = 0x00000002; // TX Enable
const E1000_TCTL_PSP: u32 = 0x00000008; // Pad Short Packets
const E1000_TCTL_CT: u32 = 0x00000FF0; // Collision Threshold (bits 7:4)
const E1000_TCTL_COLD: u32 = 0x003FF000; // Collision Distance (bits 21:12)
const E1000_TCTL_SWXOFF: u32 = 0x00400000; // Software XOFF
const E1000_TCTL_RTLC: u32 = 0x01000000; // Re-transmit on Late Collision

/// 默认冲突阈值 = 15 (标准以太网)
/// 参考: e1000_hw.c:e1000_setup_link  —  E1000_COLLISION_THRESHOLD
const E1000_CT_SHIFT: u32 = 4;
const E1000_COLLISION_THRESHOLD: u32 = 15;

/// 全双工冲突距离 = 64 (字节时间)
/// 半双工会用 512, 当前只考虑全双工链路
/// 参考: e1000_hw.c:e1000_setup_link  —  E1000_COLD
const E1000_COLD_SHIFT: u32 = 12;
const E1000_COLLISION_DISTANCE: u32 = 64;

// ============================================================================
// TX 描述符命令/状态位 (参考 Linux e1000_hw.h:634-654)
// ============================================================================

/// TX 命令位
pub const E1000_TXD_CMD_EOP: u8 = 0x01; // End Of Packet
pub const E1000_TXD_CMD_IFCS: u8 = 0x02; // Insert FCS/CRC
pub const E1000_TXD_CMD_IC: u8 = 0x04; // Insert Checksum
pub const E1000_TXD_CMD_RS: u8 = 0x08; // Report Status (set DD)
pub const E1000_TXD_CMD_RPS: u8 = 0x10; // Report Packet Sent (obsolete)
pub const E1000_TXD_CMD_DEXT: u8 = 0x20; // Descriptor Extension (0=legacy)
pub const E1000_TXD_CMD_VLE: u8 = 0x40; // VLAN Insert
pub const E1000_TXD_CMD_IDE: u8 = 0x80; // Interrupt Delay Enable

/// TX 状态位 (硬件写入 status 字段)
pub const E1000_TXD_STAT_DD: u8 = 0x01; // Descriptor Done — 发送完成
pub const E1000_TXD_STAT_EC: u8 = 0x02; // Excess Collisions
pub const E1000_TXD_STAT_LC: u8 = 0x04; // Late Collision
pub const E1000_TXD_STAT_TU: u8 = 0x08; // Transmit Underrun

/// 默认 TX 发送命令: EOP + IFCS + RS
///
/// - EOP: 单描述符包含完整帧
/// - IFCS: 硬件自动追加 CRC
/// - RS: 完成后设置 DD 状态位, 供 clean_tx_irq 回收
pub const E1000_TX_CMD_DEFAULT: u8 = E1000_TXD_CMD_EOP | E1000_TXD_CMD_IFCS | E1000_TXD_CMD_RS;

// ============================================================================
// 软件常量
// ============================================================================

/// 发送描述符个数
pub const TX_RING_COUNT: usize = 256;

/// 最小 Ethernet 帧长 (含 FCS) — 64 字节
/// 发送小于此长度的帧需要 pad
const ETHERNET_MIN_FRAME_SIZE: usize = 64;

// ============================================================================
// 数据结构 — 完全对齐 Linux 定义
// ============================================================================

/// 发送描述符 — 16 字节, 16 字节对齐
///
/// 参考 Linux: e1000_hw.h:506-520 `struct e1000_tx_desc`
/// ```c
/// struct e1000_tx_desc {
///     __le64 buffer_addr;       // 0x00 — DMA 数据缓冲区物理地址
///     union {
///         __le32 data;          // 0x08
///         struct {
///             __le16 length;    // 0x08 — 数据长度
///             __u8  cso;        // 0x0A — 校验和偏移
///             __u8  cmd;        // 0x0B — 命令 (EOP, IFCS, RS, ...)
///         } flags;
///     } lower;
///     union {
///         __le32 data;          // 0x0C
///         struct {
///             __u8  status;     // 0x0C — 状态 (DD, EC, LC, TU)
///             __u8  css;        // 0x0D — 校验和起始
///             __le16 special;   // 0x0E — VLAN 标签等
///         } fields;
///     } upper;
/// } __attribute__((aligned(16)));
/// ```
#[derive(Clone, Copy, Default)]
#[repr(C, align(16))]
pub struct E1000TxDesc {
    pub buffer_addr: u64, // 0x00 — DMA 缓冲区物理地址
    pub length: u16,      // 0x08 — 有效数据长度
    pub cso: u8,          // 0x0A — 校验和偏移 (TX 卸载用)
    pub cmd: u8,          // 0x0B — 命令字节 (EOP|IFCS|RS)
    pub status: u8,       // 0x0C — 状态 (DD 位由硬件置位)
    pub css: u8,          // 0x0D — 校验和起始
    pub special: u16,     // 0x0E — VLAN 标签等
}

const _: () = assert!(core::mem::size_of::<E1000TxDesc>() == 16);

/// 缓冲区信息 — 类似 Linux `struct e1000_tx_buffer`
///
/// 记录每个发送描述符对应的 DMA 缓冲区信息。
/// frame 持有物理页所有权, Drop 时自动释放。
/// 参考 Linux: e1000.h:136-142
pub struct E1000TxBuffer {
    /// 数据缓冲区虚拟地址
    pub data: *mut u8,
    /// 数据缓冲区物理地址 (DMA, 写入 desc.buffer_addr)
    pub dma: u64,
    /// 有效数据长度
    pub len: u16,
    /// 物理帧追踪器 — Drop 时自动释放
    pub frame: Option<FramTracker>,
}

/// 发送描述符环 — 类似 Linux `struct e1000_tx_ring`
///
/// 参考 Linux: e1000.h:165-187
pub struct E1000TxRing {
    /// 描述符环虚拟地址
    pub desc: *mut E1000TxDesc,
    /// 描述符环物理地址 (写入 TDBAL/TDBAH)
    pub dma: u64,
    /// 描述符环总字节数
    pub size: usize,
    /// 描述符个数
    pub count: usize,
    /// 下一个待使用的描述符 (软件 producer)
    pub next_to_use: usize,
    /// 下一个待回收的描述符 (软件 consumer)
    pub next_to_clean: usize,
    /// 缓冲区信息数组
    buffer_info: Vec<E1000TxBuffer>,
    /// 描述符环 DMA 物理页 — Drop 时自动释放
    desc_frames: Vec<FramTracker>,
}

impl E1000TxRing {
    /// 创建一个空的发送描述符环
    pub fn new() -> Self {
        E1000TxRing {
            desc: core::ptr::null_mut(),
            dma: 0,
            size: 0,
            count: 0,
            next_to_use: 0,
            next_to_clean: 0,
            buffer_info: Vec::new(),
            desc_frames: Vec::new(),
        }
    }
}

// ============================================================================
// 链路配置 — 参考 Linux e1000_hw.c
// ============================================================================

/// 配置链路参数 — 在硬件复位后、环形队列初始化前调用
///
/// 对应 Linux: e1000_hw.c:687 `e1000_setup_link`
///            e1000_hw.c:1879 `e1000_config_collision_dist`
///
/// ## 功能
/// 1. 强制启用链路 (CTRL.SLU)
/// 2. 配置冲突距离 (TCTL.COLLISION_DISTANCE) 基于全/半双工
/// 3. 设置帧间距 (TIPG)
/// 4. 初始化流控制寄存器 (FCT, FCAH, FCAL, FCTTV)
///
/// 参考: Linux e1000_hw.c:687-790 (e1000_setup_link)
///       Linux e1000_hw.c:1879-1895 (e1000_config_collision_dist)
///       Linux e1000_hw.h:669-672 (流控制寄存器偏移)
pub fn e1000_setup_link(bar: &BarSpace, full_duplex: bool) {
    // ── 0. 本函数所需的额外寄存器偏移 ──
    // CTRL 已在 mod.rs 中定义, 这里用局部常量以避免跨文件依赖
    const CTRL: usize = 0x00000;
    const CTRL_SLU: u32 = 0x00000040;
    const CTRL_FRCSPD: u32 = 0x00000800;
    const CTRL_FRCDPX: u32 = 0x00001000;
    // 流控制寄存器 — e1000_hw.h:669-672
    const E1000_FCT: usize = 0x00024; // Flow Control Type     (RW)
    const E1000_FCAH: usize = 0x00028; // Flow Control Addr High (RW)
    const E1000_FCAL: usize = 0x0002C; // Flow Control Addr Low  (RW)
    const E1000_FCTTV: usize = 0x00170; // Flow Control Timer     (RW)
                                        // 流控制地址常量 — e1000_hw.c:761-763
    const FLOW_CONTROL_TYPE: u32 = 0x8808;
    const FLOW_CONTROL_ADDRESS_HIGH: u32 = 0x0001;
    const FLOW_CONTROL_ADDRESS_LOW: u32 = 0x00C28001;

    // 1. Force Link Up
    //    参考: e1000_hw.c:1014-1017 (e1000_copper_link_preconfig)
    //    对于 82540EM (QEMU), 只需置位 CTRL_SLU, 让 MAC 强制认为链路已连
    //    同时清除 FRCSPD/FRCDPX, 让 MAC 自动跟随 PHY 速度/双工
    let mut ctrl = bar.read_32(CTRL);
    ctrl |= CTRL_SLU;
    ctrl &= !(CTRL_FRCSPD | CTRL_FRCDPX);
    bar.write_32(CTRL, ctrl);
    debug!("e1000: setup_link: CTRL.SLU set (CTRL={:#010x})", ctrl);

    // 2. 配置冲突距离 (TCTL.COLLISION_DISTANCE)
    //    参考: e1000_hw.c:1879-1895 e1000_config_collision_dist
    //    全双工 = 64 (0-based: 63), 半双工 = 512 (0-based: 511)
    let coll_dist: u32 = if full_duplex {
        E1000_COLLISION_DISTANCE // 64
    } else {
        512 // 半双工
    };

    let mut tctl = bar.read_32(E1000_TCTL);
    tctl &= !E1000_TCTL_COLD;
    tctl |= coll_dist << E1000_COLD_SHIFT;
    bar.write_32(E1000_TCTL, tctl);

    // 3. 设置帧间距 (Inter-Packet Gap)
    //    全双工铜线: IPGT=8 (96 bit-time), IPGR1=8, IPGR2=6
    //    参考: e1000_hw.h:2308-2322
    let tipg_ipgt = if full_duplex {
        8 // DEFAULT_82543_TIPG_IPGT_COPPER
    } else {
        10 // DEFAULT_82542_TIPG_IPGT (半双工)
    };
    let tipg = (tipg_ipgt & 0x3FF)                      // IPGT
        | ((8 as u32) & 0x3FF) << E1000_TIPG_IPGR1_SHIFT // IPGR1 = 8
        | ((6 as u32) & 0x3FF) << E1000_TIPG_IPGR2_SHIFT; // IPGR2 = 6
    bar.write_32(E1000_TIPG, tipg);

    // 4. 初始化流控制寄存器
    //    参考: e1000_hw.c:759-766
    bar.write_32(E1000_FCT, FLOW_CONTROL_TYPE);
    bar.write_32(E1000_FCAH, FLOW_CONTROL_ADDRESS_HIGH);
    bar.write_32(E1000_FCAL, FLOW_CONTROL_ADDRESS_LOW);
    bar.write_32(E1000_FCTTV, 8168); // ~100ms @ 1Gbps (Linux default)

    // FCRTL/FCRTH = 0 (禁用流控制发送暂停帧)
    // 参考: e1000_hw.c:773-775
    const E1000_FCRTL: usize = 0x00240;
    const E1000_FCRTH: usize = 0x00260;
    bar.write_32(E1000_FCRTL, 0);
    bar.write_32(E1000_FCRTH, 0);

    // 5. Write-flush: 读 STATUS 确保所有写已到达设备
    //    参考: e1000_hw.c:1894 E1000_WRITE_FLUSH
    const STATUS: usize = 0x00008;
    bar.read_32(STATUS);

    info!(
        "e1000: link setup: {} duplex, coll_dist={}, TCTL={:#010x}, TIPG={:#010x}",
        if full_duplex { "FULL" } else { "HALF" },
        coll_dist,
        tctl | (coll_dist << E1000_COLD_SHIFT),
        tipg,
    );
}

/// TIPG 寄存器位域位移
const E1000_TIPG_IPGR1_SHIFT: u32 = 10;
const E1000_TIPG_IPGR2_SHIFT: u32 = 20;

// ============================================================================
// 发送描述符环操作 — 参考 Linux e1000_main.c
// ============================================================================

/// 分配 TX 描述符环 DMA 内存和 buffer_info 数组
///
/// 对应 Linux: e1000_main.c:1758-1826 `e1000_setup_tx_resources`
///
/// ## 步骤
/// 1. 分配 buffer_info 数组
/// 2. 分配描述符环 DMA 物理连续内存
/// 3. 清零描述符环
/// 4. 初始化指针
pub fn e1000_setup_tx_resources(ring: &mut E1000TxRing, count: usize) {
    // 1. 分配 buffer_info 数组
    // 参考: e1000_main.c:1763-1770
    ring.buffer_info = Vec::with_capacity(count);
    for _ in 0..count {
        ring.buffer_info.push(E1000TxBuffer {
            data: core::ptr::null_mut(),
            dma: 0,
            len: 0,
            frame: None,
        });
    }
    ring.count = count;

    // 2. 分配描述符环 DMA 内存
    // 参考: e1000_main.c:1778-1783
    let desc_len = core::mem::size_of::<E1000TxDesc>();
    let ring_bytes = count * desc_len;
    let pages_needed = (ring_bytes + PAGE_SIZE - 1) / PAGE_SIZE;

    let frames = alloc_contiguous_frames(pages_needed)
        .expect("e1000: failed to allocate contiguous frames for tx desc ring");
    let base_pa = frames[0].ppn.0 * PAGE_SIZE;

    ring.dma = base_pa as u64;
    ring.desc = base_pa as *mut E1000TxDesc;
    ring.size = ring_bytes;
    ring.desc_frames = frames;

    info!(
        "e1000: tx ring: {} descs, {} bytes, dma={:#x}",
        ring.count, ring.size, ring.dma,
    );

    // 3. 清零描述符环
    // 参考: e1000_main.c:1819
    unsafe {
        core::ptr::write_bytes(ring.desc, 0, ring.size);
    }

    // 4. 初始化指针
    ring.next_to_clean = 0;
    ring.next_to_use = 0;
}

/// 配置硬件发送单元
///
/// 对应 Linux: e1000_main.c:1919-1968 `e1000_configure_tx`
///
/// ## 步骤
/// 1. 关闭发送引擎 (TCTL.EN=0)
/// 2. 写入 TDBAL/TDBAH/TDLEN — 描述符环基址和长度
/// 3. 初始化 TDH=0, TDT=0 — 环初始为空
/// 4. 设置 TIPG (帧间距)
/// 5. 开启发送 (TCTL: EN|PSP|CT|COLD)
pub fn e1000_configure_tx(bar: &BarSpace, ring: &E1000TxRing) {
    // 1. 关闭发送 (配置环寄存器期间防止硬件操作)
    // 参考: e1000_main.c:1934-1935
    let tctl = bar.read_32(E1000_TCTL);
    bar.write_32(E1000_TCTL, tctl & !E1000_TCTL_EN);

    // 2. + 3. 写入环基址、长度, 初始化 Head/Tail
    // 参考: e1000_main.c:1938-1945
    let tdba = ring.dma;
    bar.write_32(E1000_TDLEN, ring.size as u32);
    bar.write_32(E1000_TDBAH, (tdba >> 32) as u32);
    bar.write_32(E1000_TDBAL, (tdba & 0xFFFF_FFFF) as u32);
    bar.write_32(E1000_TDT, 0);
    bar.write_32(E1000_TDH, 0);

    // 4. 设置帧间距 (Inter-Packet Gap)
    //    全双工: IPGT=8 (96 bit time), IPGR1=10, IPGR2=12
    //    参考: e1000_hw.c:e1000_setup_link — DEFAULT_82543_TIPG_IPGT_FD
    //          e1000_hw.h:1901-1905
    let tipg = (0x00000008)      // IPGT — 全双工 96 bit time
             | (0x00000A << 10)  // IPGR1 — 10
             | (0x00000C << 20); // IPGR2 — 12
    bar.write_32(E1000_TIPG, tipg);

    info!(
        "e1000: tx configured: dma={:#x} len={} count={}",
        ring.dma, ring.size, ring.count,
    );

    // 5. 开启发送
    //    TCTL = EN | PSP | (CT << 4) | (COLD << 12)
    //    参考: e1000_main.c:1960-1963
    let tctl_config = E1000_TCTL_EN
        | E1000_TCTL_PSP
        | (E1000_COLLISION_THRESHOLD << E1000_CT_SHIFT)
        | (E1000_COLLISION_DISTANCE << E1000_COLD_SHIFT);
    bar.write_32(E1000_TCTL, tctl_config);

    info!("e1000: tx enabled (TCTL={:#010x})", tctl_config);
}

/// TX 完成回收 — 检查 DD 位释放已完成发送的缓冲区
///
/// 对应 Linux: e1000_main.c:4475-4548 `e1000_clean_tx_irq`
///
/// 比较硬件 TDH 与软件 next_to_clean, 对 DD=1 的描述符回收物理页。
///
/// ## 返回
/// 回收的描述符数量 (0 表示无完成)
///
/// ## 调用时机
/// - 中断处理程序中检测到 TXDW (TX Descriptor Done) 事件时
/// - 发送前检查是否有可用槽位时
pub fn e1000_clean_tx_irq(bar: &BarSpace, ring: &mut E1000TxRing) -> usize {
    let hw_head = bar.read_32(E1000_TDH) as usize;
    let mut i = ring.next_to_clean;
    let mut cleaned = 0;

    // 遍历所有 TDH 已经越过的描述符
    while i != hw_head {
        // SAFETY: i 在 [0, count) 范围内
        let status = unsafe { core::ptr::read_volatile(&(*ring.desc.add(i)).status) };
        if (status & E1000_TXD_STAT_DD) == 0 {
            break; // 硬件还没发送完成
        }

        // 释放 DMA 缓冲区物理页
        if let Some(frame) = ring.buffer_info[i].frame.take() {
            // frame dropped here → dealloc_frame
        }
        ring.buffer_info[i].data = core::ptr::null_mut();
        ring.buffer_info[i].dma = 0;
        ring.buffer_info[i].len = 0;

        i = (i + 1) % ring.count;
        cleaned += 1;
    }

    ring.next_to_clean = i;

    if cleaned > 0 {
        // TODO(TX): 唤醒等待发送的任务
        debug!("e1000: tx cleaned {} descriptors", cleaned);
    }

    cleaned
}

/// 提交一帧数据到 TX 描述符环
///
/// 对应 Linux: e1000_main.c:5051-5141 `e1000_xmit_frame` (极简)
///
/// ## 参数
/// - `bar`: BAR0 MMIO
/// - `ring`: TX 描述符环
/// - `data`: 待发送的帧数据
///
/// ## 返回
/// - `true`: 发送描述符已提交
/// - `false`: TX ring 已满或分配 DMA 缓冲区失败
///
/// 当前实现为每帧分配一个物理页作 DMA 缓冲区并拷贝数据。
pub fn e1000_transmit(bar: &BarSpace, ring: &mut E1000TxRing, data: NetBuffer) -> bool {
    // 环满检查: next_to_use 追上 next_to_clean 意味着无槽位
    let next = (ring.next_to_use + 1) % ring.count;
    if next == ring.next_to_clean {
        warn!(
            "e1000: tx ring full, dropping packet ({} bytes)",
            data.data_len()
        );
        return false;
    }

    if data.is_empty() || data.data_len() > 2048 {
        warn!("e1000: invalid tx packet length {}", data.data_len());
        return false;
    }

    let i = ring.next_to_use;

    // 分配物理页作 DMA 缓冲区
    let frame = match alloc_frame() {
        Some(f) => f,
        None => {
            warn!("e1000: tx OOM, dropping packet");
            return false;
        }
    };
    let pa = frame.ppn.0 * PAGE_SIZE;

    // 拷贝数据到 DMA 缓冲区 (恒等映射, PA=VA)
    unsafe {
        let slice = &mut *core::ptr::slice_from_raw_parts_mut(pa as *mut u8, data.data_len());
        slice.copy_from_slice(data.data_slice());
    }

    ring.buffer_info[i].data = pa as *mut u8;
    ring.buffer_info[i].dma = pa as u64;
    ring.buffer_info[i].len = data.data_len() as u16;
    ring.buffer_info[i].frame = Some(frame);

    // 写入描述符
    unsafe {
        (*ring.desc.add(i)).buffer_addr = pa as u64;
        (*ring.desc.add(i)).length = data.data_len() as u16;
        (*ring.desc.add(i)).cso = 0;
        (*ring.desc.add(i)).cmd = E1000_TX_CMD_DEFAULT;
        // 清零状态位 (硬件会在完成后置 DD)
        (*ring.desc.add(i)).status = 0;
    }

    // dma_wmb(): 确保描述符写入在 TDT doorbell 前全局可见
    // 参考: e1000_main.c:4656
    dma_wmb();

    // 更新 TDT 通知硬件发送
    ring.next_to_use = next;
    bar.write_32(E1000_TDT, next as u32);

    //  轮询等待硬件发送完成 (DD=1)
    //  参考: e1000_main.c 中 clean_tx_irq 在中断中完成,
    //       这里简化实现: 同步等待, 实际生产环境应使用中断
    loop {
        let status = unsafe { (*ring.desc.add(i)).status };
        if status != 0 {
            debug!(
                "e1000: tx complete desc={} len={} status={:#04x}{}{}",
                i,
                data.data_len(),
                status,
                if (status & E1000_TXD_STAT_EC) != 0 {
                    " EC"
                } else {
                    ""
                },
                if (status & E1000_TXD_STAT_LC) != 0 {
                    " LC"
                } else {
                    ""
                },
            );

            break;
        }
    }

    true
}

/// 释放 TX 描述符环资源
///
/// 对应 Linux: e1000_main.c:1949-1970 `e1000_free_tx_resources`
pub fn e1000_free_tx_resources(ring: &mut E1000TxRing) {
    ring.desc_frames.clear();
    ring.buffer_info.clear();

    ring.desc = core::ptr::null_mut();
    ring.dma = 0;
    ring.size = 0;
    ring.count = 0;
    ring.next_to_clean = 0;
    ring.next_to_use = 0;

    info!("e1000: tx ring resources freed");
}
