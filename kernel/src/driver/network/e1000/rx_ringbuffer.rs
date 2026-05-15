//! e1000 接收描述符环形队列
//!
//! 参考 Linux: drivers/net/ethernet/intel/e1000/e1000_main.c
//!
//! ## 提供的标准操作 (Linux 风格)
//!
//! | 函数 | 对应 Linux | 功能 |
//! |------|------------|------|
//! | `e1000_setup_rx_resources()` | `e1000_setup_rx_resources()` | 分配 desc ring DMA 内存和 buffer_info 数组 |
//! | `e1000_configure_rx()` | `e1000_configure_rx()` | 写入硬件寄存器, 设置 RCTL |
//! | `e1000_alloc_rx_buffers()` | `e1000_alloc_rx_buffers()` | 首次填充所有接收数据缓冲区 |
//! | `e1000_poll_rx()` | `e1000_clean_rx_irq()` 简化版 | 轮询检查 DD 位, 返回收包结果 |
//! | `e1000_refill_rx_buffers()` | `e1000_alloc_rx_buffers()` 后半段 | 补充已消费的缓冲区 |
//!
//! ## 数据流
//! ```text
//! probe:
//!   e1000_setup_rx_resources()   — 分配 desc ring + buffer_info
//!   e1000_configure_rx()          — 写入 RDBAL/RDLEN/RCTL, 使能接收
//!   e1000_alloc_rx_buffers()      — 为所有 desc 分配数据帧 + 设置 RDT
//!
//! main_loop (polling):
//!   e1000_poll_rx() → Packet     — DD=1 则返回数据指针+长度
//!   [process packet]
//!   e1000_refill_rx_buffers()    — 重新分配已消耗 desc 的数据帧
//! ```
//!
//! TODO(MMU): 物理机部署时, 描述符环和 RX 缓冲区物理页面必须带有
//!            Uncacheable (Svpbmt NC) 页表属性！

use alloc::vec::Vec;
use log::{info, warn};

use crate::driver::network::e1000::netbuffer::NetBuffer;
use crate::driver::network::e1000::{E1000, E1000_DEV};
use crate::memory::FramTracker;

// ============================================================================
// DMA 内存屏障 — RISC-V Weak Memory Order
// ============================================================================

/// DMA 读屏障: 保证所有设备内存读操作在后续读之前完成
///
/// 在 `status & DD == 1` 检查之后、读取 length/data/errors 之前必须调用。
/// 确保 CPU 在看到 DD=1 (硬件写入) 后, 读取的描述符其他字段是最新值。
///
/// 参考: e1000_main.c:4365 — `dma_rmb()`, Linux
///       arch/riscv/include/asm/barrier.h (RISC-V: RISCV_FENCE(ior, ior))
#[inline(always)]
fn dma_rmb() {
    // SAFETY: fence ir,ir 是 RISC-V 标准指令, 纯 CPU 排序, 无副作用。
    // IR = 设备输入 + 普通读; 屏障强制所有先前读在后续读之前完成。
    unsafe {
        core::arch::asm!("fence ir, ir", options(nostack, preserves_flags));
    }
}

/// DMA 写屏障: 保证所有设备内存写操作在后续写之前完成
///
/// 在 `buffer_addr` 写入描述符之后、写入 RDT doorbell 寄存器之前必须调用。
/// 确保硬件在收到 RDT 更新时看到的 buffer_addr 是最新值。
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

use crate::config::PAGE_SIZE;
use crate::driver::pcie::BarSpace;
use crate::memory::{alloc_contiguous_frames, alloc_frame};

// ============================================================================
// 寄存器偏移量 (参考 Linux e1000_hw.h)
// ============================================================================

/// RX Descriptor Base Address Low  —  e1000_hw.h:876
const E1000_RDBAL: usize = 0x02800;
/// RX Descriptor Base Address High —  e1000_hw.h:877
const E1000_RDBAH: usize = 0x02804;
/// RX Descriptor Length            —  e1000_hw.h:878
const E1000_RDLEN: usize = 0x02808;
/// RX Descriptor Head              —  e1000_hw.h:879
const E1000_RDH: usize = 0x02810;
/// RX Descriptor Tail              —  e1000_hw.h:880
const E1000_RDT: usize = 0x02818;

/// RX Control                      —  e1000_hw.h:836
const E1000_RCTL: usize = 0x00100;
/// Receive Delay Timer Register    —  e1000_hw.h:953
const E1000_RDTR: usize = 0x02820;
/// RX Checksum Control             —  e1000_hw.h:984
const E1000_RXCSUM: usize = 0x05000;

// ============================================================================
// RCTL 位定义 (参考 Linux e1000_hw.h:1811-1847)
// ============================================================================

const E1000_RCTL_EN: u32 = 0x00000002; // Receiver Enable
const E1000_RCTL_UPE: u32 = 0x00000008; // Unicast Promiscuous
const E1000_RCTL_MPE: u32 = 0x00000010; // Multicast Promiscuous
const E1000_RCTL_LBM_NO: u32 = 0x00000000; // No auto loop up test
const E1000_RCTL_LPE: u32 = 0x00000020; // Recive big packet
/// Store Bad Packet (丢弃还是保留坏包)
#[allow(dead_code)]
const E1000_RCTL_SBP: u32 = 0x00000004;
const E1000_RCTL_BAM: u32 = 0x00008000; // Broadcast Accept Mode
/// Buffer Size = 2048 (BSEX=0 时, SZ=0 表示 2048)
/// 参考: e1000_hw.h:1833
const E1000_RCTL_SZ_2048: u32 = 0x00000000;

// RXCSUM 位
/// TCP/UDP Checksum Offload Enable  —  e1000_hw.h:1968
const E1000_RXCSUM_TUOFL: u32 = 0x00000200;

// ============================================================================
// RXD 状态/错误位 (参考 Linux e1000_hw.h:563-579)
// ============================================================================

/// Descriptor Done: 硬件 DMA 完成, 数据可读
#[allow(dead_code)]
const E1000_RXD_STAT_DD: u8 = 0x01;
/// End of Packet: 单描述符收包时一定置位
#[allow(dead_code)]
const E1000_RXD_STAT_EOP: u8 = 0x02;

// ── 错误位 (errors 字段) ──
/// CRC Error                       —  e1000_hw.h:574
const E1000_RXD_ERR_CE: u8 = 0x01;
/// Symbol Error                    —  e1000_hw.h:575
#[allow(dead_code)]
const E1000_RXD_ERR_SE: u8 = 0x02;
/// Sequence Error                  —  e1000_hw.h:576
#[allow(dead_code)]
const E1000_RXD_ERR_SEQ: u8 = 0x04;
/// Carrier Extension Error         —  e1000_hw.h:577
#[allow(dead_code)]
const E1000_RXD_ERR_CXE: u8 = 0x10;
/// Rx Data Error                   —  e1000_hw.h:580
#[allow(dead_code)]
const E1000_RXD_ERR_RXE: u8 = 0x80;

/// 帧错误掩码: 组合所有需要丢弃的硬件错误位
/// 参考: e1000_hw.h:599-604 E1000_RXD_ERR_FRAME_ERR_MASK
const E1000_RXD_ERR_FRAME_ERR_MASK: u8 =
    E1000_RXD_ERR_CE | E1000_RXD_ERR_SE | E1000_RXD_ERR_SEQ | E1000_RXD_ERR_CXE | E1000_RXD_ERR_RXE;

// ============================================================================
// 软件常量
// ============================================================================

/// 接收描述符个数 (按标准 MTU=1500, 非 jumbo)
pub const RX_RING_COUNT: usize = 128;

/// Ethernet FCS/CRC 长度 (帧尾 4 字节)
/// 参考: e1000_main.c:4433 — length -= 4
const ETHERNET_FCS_LENGTH: usize = 4;

// ============================================================================
// 数据结构 — 完全对齐 Linux 定义
// ============================================================================

/// 接收描述符 (16 字节, 16 字节对齐)
///
/// 参考 Linux: e1000_hw.h:497-504 `struct e1000_rx_desc`
/// ```c
/// struct e1000_rx_desc {
///     __le64 buffer_addr;    // 8B — DMA 数据缓冲区物理地址
///     __le16 length;         // 2B — 硬件写入的实际数据长度
///     __le16 csum;           // 2B — 硬件校验和
///     u8 status;             // 1B — 状态 (DD, EOP, ...)
///     u8 errors;             // 1B — 错误标志
///     __le16 special;        // 2B — VLAN 标签等
/// } __attribute__((aligned(16)));
/// ```
#[derive(Clone, Copy, Default)]
#[repr(C, align(16))]
pub struct E1000RxDesc {
    pub buffer_addr: u64,
    pub length: u16,
    pub csum: u16,
    pub status: u8,
    pub errors: u8,
    pub special: u16,
}

const _: () = assert!(core::mem::size_of::<E1000RxDesc>() == 16);

/// 缓冲区信息 — 类似 Linux `struct e1000_rx_buffer`
///
/// 每个描述符对应一个 E1000RxBuffer, 记录数据缓冲区的虚拟地址和 DMA 地址。
/// frame 持有物理页所有权，Drop 时自动释放。
/// 参考 Linux: e1000.h:136-142
struct E1000RxBuffer {
    /// 数据缓冲区虚拟地址 (CPU 读写)
    data: *mut u8,
    /// 数据缓冲区物理地址 (DMA, 写入 desc.buffer_addr)
    dma: u64,
    /// 物理帧追踪器 — Drop 时自动释放物理页
    frame: Option<FramTracker>,
}

/// 接收描述符环 — 类似 Linux `struct e1000_rx_ring`
///
/// 参考 Linux: e1000.h:165-187
pub struct E1000RxRing {
    /// 描述符环虚拟地址
    pub desc: *mut E1000RxDesc,
    /// 描述符环物理地址 (写入 RDBAL/RDBAH)
    pub dma: u64,
    /// 描述符环总字节数
    pub size: usize,
    /// 描述符个数
    pub count: usize,
    /// 下一个待使用的描述符 (软件写入新的 buffer_addr)
    pub next_to_use: usize,
    /// 下一个待检查的描述符 (软件检查 DD 位)
    pub next_to_clean: usize,
    /// 缓冲区信息数组 [`count` 项]
    buffer_info: Vec<E1000RxBuffer>,
    /// 描述符环 DMA 物理页 — Drop 时自动释放
    desc_frames: Vec<FramTracker>,
}

impl E1000RxRing {
    /// 创建一个空的接收描述符环 (需要调用 `e1000_setup_rx_resources` 分配内存)
    pub fn new() -> Self {
        E1000RxRing {
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
// 描述符环操作函数 — 参考 Linux e1000_main.c
// ============================================================================

/// 分配接收描述符环内存和 buffer_info 数组
///
/// 对应 Linux: e1000_main.c:1682-1746 `e1000_setup_rx_resources`
///
/// ## 步骤
/// 1. 分配 buffer_info 数组 (vzalloc 等效)
/// 2. 分配描述符环 DMA 物理连续内存 (dma_alloc_coherent 等效)
/// 3. 清零描述符环
/// 4. 初始化指针
///
/// ## 注意
/// 分配后需在适当的时机调用 `e1000_free_rx_resources()` 释放。
/// 当前使用恒等映射 (物理地址 == 虚拟地址), 适用于 QEMU。
pub fn e1000_setup_rx_resources(ring: &mut E1000RxRing, count: usize) {
    // 1. 分配 buffer_info 数组
    // 对应 Linux: rxdr->buffer_info = vzalloc(size * sizeof(struct e1000_rx_buffer))
    // 参考: e1000_main.c:1688-1691
    ring.buffer_info = Vec::with_capacity(count);
    for _ in 0..count {
        ring.buffer_info.push(E1000RxBuffer {
            data: core::ptr::null_mut(),
            dma: 0,
            frame: None,
        });
    }
    ring.count = count;

    // 2. 分配描述符环 DMA 内存
    // 对应 Linux: rxdr->desc = dma_alloc_coherent(&pdev->dev, size, &rxdr->dma, GFP_KERNEL)
    // 参考: e1000_main.c:1700-1701
    //
    // dma_alloc_coherent 返回物理连续、一致缓存的内存。
    // 当前用 alloc_contiguous_frames 替代, 且 QEMU 无需真正 uncacheable。
    let desc_len = core::mem::size_of::<E1000RxDesc>();
    let ring_bytes = count * desc_len;
    let pages_needed = (ring_bytes + PAGE_SIZE - 1) / PAGE_SIZE;

    let frames = alloc_contiguous_frames(pages_needed)
        .expect("e1000: failed to allocate contiguous frames for rx desc ring");
    let base_pa = frames[0].ppn.0 * PAGE_SIZE;

    ring.dma = base_pa as u64;
    // 当前物理内存被恒等映射 (PA == VA)
    ring.desc = base_pa as *mut E1000RxDesc;
    ring.size = ring_bytes;

    // 持有 FrameTracker 所有权, Drop 时自动释放物理页
    ring.desc_frames = frames;

    info!(
        "e1000: rx ring: {} descs, {} bytes, dma={:#x}, virt={:p}",
        ring.count, ring.size, ring.dma, ring.desc,
    );

    // 3. 清零描述符环 (对应 Linux: memset(rxdr->desc, 0, rxdr->size))
    // 参考: e1000_main.c:1739
    unsafe {
        core::ptr::write_bytes(ring.desc, 0, ring.size);
    }

    // 4. 初始化指针 (对应 Linux: e1000_main.c:1741-1743)
    ring.next_to_clean = 0;
    ring.next_to_use = 0;
}

/// 配置硬件接收单元
///
/// 对应 Linux: e1000_main.c:1846-1909 `e1000_configure_rx`
///
/// ## 步骤
/// 1. 关闭接收引擎 (RCTL.EN=0) — 配置环寄存器期间防止硬件误操作
/// 2. 写入 RDBAL/RDBAH/RDLEN — 描述符环基址和长度
/// 3. 初始化 RDH=0, RDT=0 — 环初始为空 (Tail=Head 表示空)
/// 4. 设置 RDTR=0 (polling 模式) + 使能 RXCSUM 卸载 + 设 RCTL 含 BSIZE
///
/// ## 注意
/// 必须在 `e1000_setup_rx_resources()` 之后、`e1000_alloc_rx_buffers()` 之前调用。
pub fn e1000_configure_rx(bar: &BarSpace, ring: &E1000RxRing) {
    // 1. 关闭接收 (对应 Linux: ew32(RCTL, rctl & ~E1000_RCTL_EN))
    //    参考: e1000_main.c:1865-1866
    //  避免配置期间存在的旧配置接受到包继续发dma
    let rctl = bar.read_32(E1000_RCTL);
    bar.write_32(E1000_RCTL, rctl & !E1000_RCTL_EN);

    // 2. + 3. 写入环基址、长度, 初始化 Head/Tail
    //    参考: e1000_main.c:1883-1893
    let rdba = ring.dma;
    bar.write_32(E1000_RDLEN, ring.size as u32);
    bar.write_32(E1000_RDBAH, (rdba >> 32) as u32);
    bar.write_32(E1000_RDBAL, (rdba & 0xFFFF_FFFF) as u32);
    bar.write_32(E1000_RDT, 0);
    bar.write_32(E1000_RDH, 0);

    // 设置 Receive Delay Timer = 0 (polling 模式, 无中断延迟)
    // 参考: e1000_main.c:1869 — ew32(RDTR, adapter->rx_int_delay)
    // 实现中断合并：通过强制硬件在收到帧后延迟一段时间再产生中
    // 断，让多个帧在一次中断内被批量处理，从而降低中断频率和 CPU 开销，提升吞吐量。
    bar.write_32(E1000_RDTR, 0);

    // TODO: 82540+ 的中断绝对延迟（写 RADV）
    // 设定一个绝对超时值，防止在流量极低时，因 RDTR 定时器不断被新包
    // 重置而导致已收到的帧被无限期滞留。确保即使流量稀疏，帧也能
    // 在确定延迟内被处理，满足低延迟需求

    // TODO:中断节流率（写 ITR，仅 82540+ 且 itr_setting != 0）
    // 启用硬件级动态中断调节：硬件依据目标中断速率
    // （例如每秒 8000 次）自动调整实际中断间隔，在吞吐和延迟之间动态平衡，无需驱动频繁干预。

    // 启用 82543+ TCP/UDP 校验和卸载
    // 参考: e1000_main.c:1897-1904 — rxcsum |= E1000_RXCSUM_TUOFL
    bar.write_32(E1000_RXCSUM, E1000_RXCSUM_TUOFL);

    info!(
        "e1000: rx configured: dma={:#x} len={} count={}",
        ring.dma, ring.size, ring.count,
    );

    // 4. 开启接收 (设置 RCTL: EN + 混杂模式 + BSIZE=2048)
    //    参考: e1000_main.c:1908
    let rctl_config = E1000_RCTL_EN
        | E1000_RCTL_BAM //接受广播 帧
        | E1000_RCTL_UPE //
        | E1000_RCTL_MPE
        | E1000_RCTL_LBM_NO
        | E1000_RCTL_LPE
        | E1000_RCTL_SBP //收下坏包
        | E1000_RCTL_SZ_2048; // 显式设 buffer size = 2048
    bar.write_32(E1000_RCTL, rctl_config);

    info!("e1000: rx enabled (RCTL={:#010x})", rctl_config);
}

/// 首次填充所有接收数据缓冲区
///
/// 对应 Linux: e1000_main.c:4550-4659 `e1000_alloc_rx_buffers`
///
/// 为每个描述符分配一个物理页作为数据缓冲区, 将 DMA 地址写入 desc.buffer_addr,
/// 然后设置 RDT 通知硬件开始接收。
pub fn e1000_alloc_rx_buffers(bar: &BarSpace, ring: &mut E1000RxRing) {
    let mut i = ring.next_to_use;

    // 填充所有描述符
    for _ in 0..ring.count {
        // 跳过已经分配了缓冲区的槽位 (首次全部为空, 后续 refill 会有已分配的)
        if !ring.buffer_info[i].data.is_null() {
            i = (i + 1) % ring.count;
            continue;
        }

        // 分配一个物理页作为数据缓冲区
        // 对应 Linux: data = e1000_alloc_frag(adapter) → netdev_alloc_frag → page
        // 参考: e1000_main.c:4570
        //
        // TODO: 这里应该使用 DMA 池或 skb_page 分配, 当前简单用 alloc_frame.
        // TODO: 在未取消恒等映射前, data = dma = PA 可用.
        let frame = alloc_frame().expect("e1000: OOM during rx buffer allocation");
        let pa = frame.ppn.0 * PAGE_SIZE;

        ring.buffer_info[i].data = pa as *mut u8;
        ring.buffer_info[i].dma = pa as u64;
        // 持有 FramTracker, 旧页自动释放, 无 forget
        ring.buffer_info[i].frame = Some(frame);

        // buffer_addr = 缓冲区物理地址 (小端)
        // 参考: e1000_main.c:4639
        unsafe {
            (*ring.desc.add(i)).buffer_addr = pa as u64;
        }

        i = (i + 1) % ring.count;
    }

    ring.next_to_use = i;

    // dma_wmb(): 保证所有 buffer_addr 写入在 RDT doorbell 之前全局可见
    // 参考: e1000_main.c:4656
    dma_wmb();

    // 设置 RDT = (next_to_use - 1) mod count
    // 告诉硬件: 描述符 0 到 RDT 都有有效 buffer_addr, 可以 DMA
    // 参考: e1000_main.c:4646-4657
    let rdt = if i == 0 { ring.count - 1 } else { i - 1 };
    bar.write_32(E1000_RDT, rdt as u32);

    info!(
        "e1000: rx buffers allocated: count={}, RDT={}",
        ring.count, rdt,
    );
}

/// 轮询收包 — 检查 DD 位, 返回接收数据
///
/// 对应 Linux: e1000_main.c:4339 `e1000_clean_rx_irq` (简化轮询版, 无 NAPI)
///
/// ## 返回
/// - `Some(pkt)`: 收到一个完整、无错误的数据帧 (已剥离 FCS)
/// - `None`: 无新数据 / 帧有错误 / jumbo 帧
///
/// ## 注意
/// 该函数不重新分配缓冲区。调用者读取 pkt 内容后必须调用
/// `e1000_refill_rx_buffers()` 回收描述符。
///
#[allow(dead_code)]
pub fn e1000_poll_rx() -> Option<NetBuffer> {
    let mut lock = E1000_DEV.try_lock().expect("Lock is busy or get not exist");
    let ring = &mut (*lock).as_mut().expect("nodev").rx_ring;
    let i = ring.next_to_clean;

    // 读描述符状态 (volatile 防止编译器优化)
    // 参考: e1000_main.c:4357
    let status = unsafe { core::ptr::read_volatile(&(*ring.desc.add(i)).status) };
    if (status & E1000_RXD_STAT_DD) == 0 {
        return None; // 无新包
    }

    // dma_rmb(): 确保读取 DD=1 后, length/errors/data 是硬件 DMA 写完的最新值
    // 在 RISC-V 弱序模型下, CPU 可能先看到 DD=1 但 length 还没写完成。
    // 参考: e1000_main.c:4365
    dma_rmb();

    // 读取长度 (硬件报告的 length 包含 4 字节 FCS)
    // 参考: e1000_main.c:4368
    let length = unsafe { core::ptr::read_volatile(&(*ring.desc.add(i)).length) } as usize;
    let data = ring.buffer_info[i].data as *const u8;

    // ── !EOP: 多描述符帧 (jumbo) ──
    // 当前不处理 jumbo 帧, 仅清除当前 desc 的 DD, 返回 None。
    // 后续 poll 会逐个跳过剩下的分片 desc 直到 EOP, 然后硬件恢复收包。
    // 参考: e1000_main.c:4407-4417 (Linux 用 adapter->discarding 标记跳过整帧)
    if (status & E1000_RXD_STAT_EOP) == 0 {
        unsafe {
            (*ring.desc.add(i)).status = 0;
        }
        ring.next_to_clean = (i + 1) % ring.count;
        warn!("e1000: dropped multi-descriptor frame (jumbo not supported)");
        return None;
    }

    // ── 帧错误检查 ──
    // 检查 CRC/符号/序列/载波扩展/Rx 数据错误。
    // 参考: e1000_main.c:4419-4429
    let errors = unsafe { core::ptr::read_volatile(&(*ring.desc.add(i)).errors) };
    if (errors & E1000_RXD_ERR_FRAME_ERR_MASK) != 0 {
        // 有硬件错误, 丢弃此帧
        // 在完整驱动中此处应统计 rx_crc_errors 等计数器
        unsafe {
            (*ring.desc.add(i)).status = 0;
        }
        ring.next_to_clean = (i + 1) % ring.count;
        return None;
    }

    // ── FCS 剥离 ──
    // e1000 硬件在标准模式下不自动剥离 FCS, length 包含 4 字节 Ethernet CRC。
    // Linux 通过 length -= 4 手动剥离 (除非 NETIF_F_RXFCS 保留原始帧)。
    // 参考: e1000_main.c:4433,4440
    let pkt_len = length - ETHERNET_FCS_LENGTH;

    // 清除 DD 位, 标记描述符已消费
    // 参考: e1000_main.c:4456
    unsafe {
        (*ring.desc.add(i)).status = 0;
    }
    ring.next_to_clean = (i + 1) % ring.count;
    let mut new_buffer = NetBuffer::new();
    new_buffer.new_data(unsafe { &*core::ptr::slice_from_raw_parts(data, pkt_len) });
    Some(new_buffer)
}

/// 补充已消费的接收缓冲区
///
/// 对应 Linux: e1000_main.c:4550-4659 `e1000_alloc_rx_buffers` (每次 poll 后调用)
///
/// 从 `next_to_use` 到 `next_to_clean` 的区间重新分配数据帧, 刷新 buffer_addr,
/// 并更新 RDT 通知硬件。
///
/// ## 调用时机
/// 处理完 `e1000_poll_rx()` 返回的收包结果后调用。
/// 不调用会导致描述符耗尽, 硬件无法继续收包。
///
/// ## 注意
/// 当分配失败时, 跳过当前槽位, 下次 refill 重试。
/// TODO(alloc): 物理机部署时, 此处分配失败不应 panic, 应更新丢弃计数
#[allow(dead_code)]
pub fn e1000_refill_rx_buffers(bar: &BarSpace, ring: &mut E1000RxRing) {
    let mut i = ring.next_to_use;
    let mut refilled = 0;

    // 判断是否需要 refill: (clean - use - 1) mod count > 0 表示有空位
    // 对应 Linux: e1000.h:189-194 E1000_DESC_UNUSED
    let clean = ring.next_to_clean;
    let unused = if clean > i {
        clean - i - 1
    } else {
        ring.count + clean - i - 1
    };
    if unused == 0 {
        return;
    }

    // 批量 refill
    for _ in 0..unused {
        // 分配新帧
        let frame = match alloc_frame() {
            Some(f) => f,
            None => {
                // TODO: 增加 e1000 驱动丢包计数 stats.rx_alloc_failed
                warn!("e1000: rx refill OOM, skipping desc {}", i);
                break;
            }
        };
        let pa = frame.ppn.0 * PAGE_SIZE;

        ring.buffer_info[i].data = pa as *mut u8;
        ring.buffer_info[i].dma = pa as u64;
        // 持有 FramTracker, 旧页自动释放, 无 forget
        ring.buffer_info[i].frame = Some(frame);

        unsafe {
            (*ring.desc.add(i)).buffer_addr = pa as u64;
        }

        i = (i + 1) % ring.count;
        refilled += 1;
    }

    if refilled == 0 {
        return;
    }

    ring.next_to_use = i;

    // dma_wmb(): 保证所有 buffer_addr 写入在 RDT doorbell 之前全局可见
    // 参考: e1000_main.c:4656
    dma_wmb();

    // 写 RDT = 最后一个有效描述符索引 (= next_to_use - 1)
    // 对应 Linux: e1000_main.c:4657 writel(i, hw->hw_addr + rx_ring->rdt)
    let rdt = if i == 0 { ring.count - 1 } else { i - 1 };
    bar.write_32(E1000_RDT, rdt as u32);
}

/// 释放所有描述符环资源
///
/// 对应 Linux: e1000_main.c:1918-1932 `e1000_free_rx_resources`
///
/// desc_frames 和 buffer_info 的 Drop 自动释放物理页。
/// 调用后 ring 不再可用。
#[allow(dead_code)]
pub fn e1000_free_rx_resources(ring: &mut E1000RxRing) {
    // desc_frames Drop 释放描述符环 DMA 内存
    ring.desc_frames.clear();
    // buffer_info Drop 释放所有数据缓冲区物理页
    ring.buffer_info.clear();

    ring.desc = core::ptr::null_mut();
    ring.dma = 0;
    ring.size = 0;
    ring.count = 0;
    ring.next_to_clean = 0;
    ring.next_to_use = 0;

    info!("e1000: rx ring resources freed");
}
