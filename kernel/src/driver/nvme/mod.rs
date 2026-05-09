//! NVMe PCIe 驱动骨架。
//!
//! 这个模块现在**不是可运行驱动**，而是给后续实现准备的“带语义类型的施工图”。
//! 你后面应该直接在这里把控制器 bring-up、admin queue、IO queue、PRP 和
//! `BlockDevTrait` 实现补完整，而不是重新散落到别的文件。
//!
//! ## BlueStarOS 里的接入位置
//!
//! 1. `driver/pcie/mod.rs` 会先扫描 ECAM，并把每个 BDF 注册进 `PCIE_DEVICES`；
//! 2. NVMe 驱动随后从 `PCIE_DEVICES` 里找 class code = `01h/08h/02h` 的控制器；
//! 3. 驱动完成 BAR0 + queue bring-up 后，构造 `NvmeBlockDevice`；
//! 4. 最后通过 `register_global_block_device()` 注册进 VFS；
//! 5. `RootFs::init_rootfs()` 不需要理解 NVMe，它只消费 `GLOBAL_BLOCKS`。
//!
//! ## 第一版目标边界
//!
//! 第一版只做下面这些，先把“能挂根文件系统的块设备”跑通：
//! 1. PCIe 端点匹配；
//! 2. BAR0 MMIO；
//! 3. `PCI_COMMAND_MEMORY | PCI_COMMAND_MASTER`；
//! 4. 控制器 reset / enable；
//! 5. 1 对 Admin Queue，轮询完成；
//! 6. `Identify Controller`；
//! 7. `Identify Namespace`；
//! 8. 1 对 I/O Queue，轮询完成；
//! 9. 只做 PRP，不做 SGL；
//! 10. 先只支持 namespace 1；
//! 11. 先只支持 4KiB 内单页读写，后面再扩展跨页 PRP list。
//!
//! ## Linux 5.4.29 参考
//!
//! 必看文件与行号：
//! - `include/linux/nvme.h:89-166`：AQ 深度、寄存器偏移、CAP/CC/CSTS 位定义
//! - `include/linux/nvme.h:672-976`：命令格式（common / rw / identify / create cq / create sq）
//! - `include/linux/nvme.h:1377-1390`：Completion Queue Entry 格式
//! - `drivers/nvme/host/pci.c:90-210`：`nvme_dev` / `nvme_queue` 的最小字段集合
//! - `drivers/nvme/host/pci.c:1116-1167`：Create CQ / SQ admin command 填法
//! - `drivers/nvme/host/pci.c:1690-1711`：Admin Queue 初始化，写 `AQA/ASQ/ACQ`
//! - `drivers/nvme/host/pci.c:1729-1764`：创建 I/O queues 的顺序
//! - `drivers/nvme/host/pci.c:795-843`：数据映射入口，第一版先只学 PRP 部分
//! - `drivers/nvme/host/pci.c:862-908`：提交一个 I/O 请求的最小主路径
//! - `drivers/nvme/host/core.c:2060-2147`：`CC.EN` 开关与 `CSTS.RDY` 等待流程
//!
//! 这里故意不照搬 Linux 的 blk-mq / MSI-X / 多队列复杂度，
//! 但**控制器状态机、命令格式、队列创建顺序**必须按规范来。
use core::arch::asm;
use core::ptr::read_volatile;
use core::sync::atomic::{AtomicU16, Ordering};

use alloc::{sync::Arc, vec::Vec};
use log::{debug, error, info};
use spin::Mutex;

use crate::arch::memory::PhysiAddr;
use crate::config::{PAGE_SIZE, SECTOR_SIZE};
use crate::driver::nvme::cst::nvme_regs::{
    NVME_REG_ACQ, NVME_REG_AQA, NVME_REG_ASQ, NVME_REG_CAP, NVME_REG_CC, NVME_REG_CSTS,
    NVME_REG_DOORBELL_BASE, NVME_REG_VS,
};
use crate::driver::pcie::{
    cfg_read16, cfg_write16, collect_pcie_devices_by_target, BarSpace, PcieBarSpace,
    PcieDeviceInfo, PcieDeviceTarget, PCI_COMMAND, PCI_COMMAND_MASTER, PCI_COMMAND_MEMORY,
};
use crate::error::BlueErr;
use crate::fs::vfs::{register_global_block_device, BlockDevTrait, VfsFsError};
use crate::memory::{alloc_contiguous_frames, FramTracker};
use crate::time::kernel_sleep;
mod cst;
/// NVMe PCI class code：Base Class = 0x01, Sub Class = 0x08, Prog IF = 0x02。
///
/// 参考 Linux 5.4.29:
/// - `drivers/nvme/host/pci.c:3143-3148`
pub const NVME_PCI_CLASS_CODE: u32 = 0x01_08_02;

/// 控制器寄存器一般使用 BAR0 暴露。
pub const NVME_CONTROLLER_BAR_INDEX: u8 = 0;

/// NVMe Identify Namespace 的 `FLBAS` 低 4 位：当前 active LBA format 下标。
///
/// 参考 Linux 5.4.29:
/// - `include/linux/nvme.h:300-341`：`struct nvme_id_ns` / `struct nvme_lbaf`；
/// - `drivers/nvme/host/core.c:1830`：`id->flbas & NVME_NS_FLBAS_LBA_MASK`。
pub const NVME_NS_FLBAS_LBA_MASK: u8 = 0x0f;

/// NVMe Namespace 没有给出有效 `LBAF.ds` 时的兜底 shift。
///
/// 参考 Linux 5.4.29:
/// - `drivers/nvme/host/core.c:1830-1832`：`lba_shift == 0` 时按 512B 处理。
pub const NVME_DEFAULT_LBA_SHIFT: u8 = 9;

/// 第一版 admin queue 固定深度。
///
/// 参考 Linux 5.4.29:
/// - `include/linux/nvme.h:89`
pub const NVME_ADMIN_QUEUE_DEPTH: u16 = 32;

/// 第一版 IO queue 的目标深度。
///
/// TODO(dirinkbottle):
/// 真正初始化时不要直接信这个常量，应该：
/// 1. 先读取 `CAP.MQES`；
/// 2. 用 `min(CAP.MQES + 1, NVME_IO_QUEUE_DEPTH)`；
/// 3. 最后把结果写进 `AQA` / Create SQ/CQ 命令。
pub const NVME_IO_QUEUE_DEPTH: u16 = 64;

/// SQE 大小编码：64B。
///
/// 参考 Linux 5.4.29:
/// - `include/linux/nvme.h:159-165`
pub const NVME_CC_IOSQES_64B: u32 = 6;

/// CQE 大小编码：16B。
///
/// 参考 Linux 5.4.29:
/// - `include/linux/nvme.h:159-165`
pub const NVME_CC_IOCQES_16B: u32 = 4;

/// NVMe PCIe 设备筛选器。
///
/// 当前 BlueStarOS 已经把 PCIe 扫描结果缓存进 `PCIE_DEVICES`，
/// 因此 NVMe 驱动的第一步不是直接扫 ECAM，而是从缓存里筛出目标控制器。
pub struct NvmePcieDeviceTarget;

impl PcieDeviceTarget for NvmePcieDeviceTarget {
    fn matches(device: &PcieDeviceInfo) -> bool {
        device.class_code == NVME_PCI_CLASS_CODE
    }
}

/// NVMe 队列 ID。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct NvmeQueueId(pub u16);

impl NvmeQueueId {
    /// Admin queue 固定使用 QID 0。
    pub const ADMIN: Self = Self(0);
    /// 第一对 I/O queue 从 QID 1 开始。
    pub const IO0: Self = Self(1);
}

/// NVMe 命令 ID。
///
/// TODO(dirinkbottle):
/// 第一版先做一个简单的单调递增分配器，轮询模式下不用太复杂；
/// 等中断和并发队列支持起来后，再把 CID 生命周期管理细化。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NvmeCommandId(pub u16);

/// NVMe Namespace ID。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NvmeNamespaceId(pub u32);

impl NvmeNamespaceId {
    /// 第一版先只实现 namespace 1。
    pub const PRIMARY: Self = Self(1);
}

/// NVMe 逻辑块地址。
///
/// 用 newtype 把“这是 LBA，不是普通 u64”表达出来，后面读写路径更不容易把
/// 字节偏移、页号、LBA 混在一起。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NvmeLogicalBlockAddress(pub u64);

/// NVMe 逻辑块数量。
///
/// NVMe Read/Write 命令里的 `length` 字段是 zero-based NLB：
/// `block_count=1` 要写成 `length=0`。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NvmeLogicalBlockCount(pub u16);

impl NvmeLogicalBlockCount {
    /// 转成 NVMe 命令里的 zero-based NLB 字段。
    pub fn to_zero_based_nlb(self) -> u16 {
        self.0.saturating_sub(1)
    }
}

/// NVMe doorbell stride。
///
/// CAP.DSTRD 存的是指数：每个 doorbell register 的步进是 `4 << DSTRD` 字节。
/// Linux 5.4.29 参考：
/// - `include/linux/nvme.h:124` 的 `NVME_CAP_STRIDE(cap)`。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NvmeDoorbellStride {
    pub bytes: u32,
}

impl NvmeDoorbellStride {
    /// 从 CAP 寄存器提取 DSTRD。
    pub fn from_capability(capability: u64) -> Self {
        let dstrd = ((capability >> 32) & 0x0f) as u32;
        Self { bytes: 4 << dstrd }
    }

    /// SQ tail doorbell offset：DBS + (2 * qid) * stride。
    pub fn submission_tail_offset(self, queue_id: NvmeQueueId, doorbell_base_offset: u32) -> usize {
        doorbell_base_offset as usize + (2 * queue_id.0 as usize) * self.bytes as usize
    }

    /// CQ head doorbell offset：DBS + (2 * qid + 1) * stride。
    pub fn completion_head_offset(self, queue_id: NvmeQueueId, doorbell_base_offset: u32) -> usize {
        doorbell_base_offset as usize + (2 * queue_id.0 as usize + 1) * self.bytes as usize
    }
}

impl Default for NvmeDoorbellStride {
    fn default() -> Self {
        // QEMU NVMe 常见 DSTRD=0，即每个 doorbell register 间隔 4 字节。
        Self { bytes: 4 }
    }
}

/// NVMe Admin Opcode。
///
/// 参考 Linux 5.4.29:
/// - `include/linux/nvme.h:795-819`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum NvmeAdminOpcode {
    CreateSubmissionQueue = 0x01,
    CreateCompletionQueue = 0x05,
    Identify = 0x06,
}

/// NVMe I/O Opcode。
///
/// Linux 5.4.29 参考：
/// - `include/linux/nvme.h:559-561`：`flush/write/read` opcode；
/// - `drivers/nvme/host/core.c:600-605`：Flush 命令只需要 opcode + namespace id。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum NvmeIoOpcode {
    Flush = 0x00,
    Write = 0x01,
    Read = 0x02,
}

/// CC 寄存器位定义。
///
/// 参考 Linux 5.4.29:
/// - `include/linux/nvme.h:148-165`
pub mod cc {
    pub const ENABLE: u32 = 1 << 0;
    pub const CSS_NVM: u32 = 0 << 4;
    pub const MPS_SHIFT: u32 = 7;
    pub const AMS_ROUND_ROBIN: u32 = 0 << 11;
    pub const IOSQES_SHIFT: u32 = 16;
    pub const IOCQES_SHIFT: u32 = 20;
}

/// CSTS 寄存器位定义。
///
/// 参考 Linux 5.4.29:
/// - `include/linux/nvme.h:166-172`
pub mod csts {
    pub const RDY: u32 = 1 << 0;
    pub const CFS: u32 = 1 << 1;
}

/// PRP 数据指针。
///
/// 第一版只做 PRP，不做 SGL，因此数据指针先展开为 `prp1/prp2` 两个字段。
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct NvmePrpDataPointer {
    /// 第一段数据页物理地址。
    pub prp1: u64,
    /// 第二段数据页物理地址，或 PRP List 页地址。
    pub prp2: u64,
}

/// NVMe Read / Write 命令格式。
///
/// 参考 Linux 5.4.29:
/// - `include/linux/nvme.h:688-703`
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct NvmeRwCommand {
    pub opcode: u8,
    pub flags: u8,
    pub command_id: u16,
    pub namespace_id: u32,
    pub reserved2: u64,
    pub metadata_pointer: u64,
    pub data_pointer: NvmePrpDataPointer,
    pub start_lba: u64,
    /// 零基 NLB。
    ///
    /// 例子：
    /// - 读 1 个逻辑块 -> `length = 0`
    /// - 读 8 个 512B 逻辑块（即 4KiB）-> `length = 7`
    pub length: u16,
    pub control: u16,
    pub dsmgmt: u32,
    pub reftag: u32,
    pub apptag: u16,
    pub appmask: u16,
}

/// Identify 命令格式。
///
/// 参考 Linux 5.4.29:
/// - `include/linux/nvme.h:904-915`
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct NvmeIdentifyCommand {
    pub opcode: u8,
    pub flags: u8,
    pub command_id: u16,
    pub namespace_id: u32,
    pub reserved2: [u64; 2],
    pub data_pointer: NvmePrpDataPointer,
    pub controller_or_namespace_structure: u8,
    pub reserved3: u8,
    pub controller_id: u16,
    pub reserved11: [u32; 5],
}

/// Create Completion Queue 命令格式。
///
/// 参考 Linux 5.4.29:
/// - `include/linux/nvme.h:940-952`
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct NvmeCreateCompletionQueueCommand {
    pub opcode: u8,
    pub flags: u8,
    pub command_id: u16,
    pub reserved1: [u32; 5],
    pub prp1: u64,
    pub reserved8: u64,
    pub completion_queue_id: u16,
    pub queue_size_zero_based: u16,
    pub completion_queue_flags: u16,
    pub interrupt_vector: u16,
    pub reserved12: [u32; 4],
}

/// Create Submission Queue 命令格式。
///
/// 参考 Linux 5.4.29:
/// - `include/linux/nvme.h:954-966`
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct NvmeCreateSubmissionQueueCommand {
    pub opcode: u8,
    pub flags: u8,
    pub command_id: u16,
    pub reserved1: [u32; 5],
    pub prp1: u64,
    pub reserved8: u64,
    pub submission_queue_id: u16,
    pub queue_size_zero_based: u16,
    pub submission_queue_flags: u16,
    pub completion_queue_id: u16,
    pub reserved12: [u32; 4],
}

pub static QUEUE_COMMAND_ID: AtomicU16 = AtomicU16::new(0);

/// 分配一个轮询路径使用的命令 ID。
fn next_command_id() -> NvmeCommandId {
    NvmeCommandId(QUEUE_COMMAND_ID.fetch_add(1, Ordering::SeqCst))
}

/// 把一个 64B SQE 写入任意 SQ slot。
///
/// Linux 5.4.29 `drivers/nvme/host/pci.c:476-484` 的主流程就是：
/// 1. `memcpy` 命令到 `sq_tail` 指向的 SQE；
/// 2. 推进 tail；
/// 3. `writel` tail 到 SQ doorbell。
///
/// 这里只负责第 1 步，tail 和 doorbell 由提交函数维护。
unsafe fn write_queue_sqe<Command>(
    submission_queue_dma_base: PhysiAddr,
    slot_index: usize,
    command: &Command,
) {
    assert!(core::mem::size_of::<Command>() == 64);
    let dst = (submission_queue_dma_base.0 + slot_index * 64) as *mut u8;
    let src = command as *const Command as *const u8;
    unsafe {
        core::ptr::copy_nonoverlapping(src, dst, 64);
    }
}

/// 轮询 Admin CQ 的一个 CQE，并打印该命令的完成状态。
///
/// Linux 5.4.29 `drivers/nvme/host/pci.c:924-928`：
/// completion 是否有效看 `status & 1`，也就是 phase tag，不是 bit15。
unsafe fn poll_admin_cqe(
    bar0_space: &BarSpace,
    admin_queue: &mut NvmeQueueState,
    operation_name: &str,
) -> NvmeCompletionQueueEntry {
    let completion_index = admin_queue.completion_head as usize;
    let expected_phase = admin_queue.completion_phase as u16;
    let target = (admin_queue.completion_queue_dma_base.0 as *const NvmeCompletionQueueEntry)
        .add(completion_index);
    let mut cqe = unsafe { read_volatile(target) };
    let mut spin_count = 0usize;

    while (cqe.status & 1) != expected_phase {
        spin_count += 1;
        if spin_count % 1_000_000 == 0 {
            debug!(
                "wait {} cqe[{}]: spin={} status={:#x} sq_id={} cid={}",
                operation_name,
                completion_index,
                spin_count,
                cqe.status,
                cqe.submission_queue_id,
                cqe.command_id
            );
        }
        cqe = unsafe { read_volatile(target) };
    }

    let nvme_status = cqe.status >> 1;
    if nvme_status == 0 {
        info!("NVMe admin {} success: cqe={:?}", operation_name, cqe);
    } else {
        error!(
            "NVMe admin {} failed: cqe={:?} nvme_status={:#x}",
            operation_name, cqe, nvme_status
        );
    }

    // 消费一个 CQE 后推进 head。到达队列末尾时 phase tag 翻转。
    //
    // Linux 5.4.29 `drivers/nvme/host/pci.c:986-993`:
    // head wrap 时 `cq_phase = !cq_phase`。
    admin_queue.completion_head += 1;
    if admin_queue.completion_head == admin_queue.queue_depth {
        admin_queue.completion_head = 0;
        admin_queue.completion_phase ^= 1;
    }

    // Admin CQ0 head doorbell:
    // DBS + (2 * qid + 1) * stride_bytes；stride 已经由 CAP.DSTRD 解析得到。
    let cq_head_doorbell_offset = admin_queue
        .doorbell_stride
        .completion_head_offset(admin_queue.queue_id, admin_queue.doorbell_base_offset);
    bar0_space.write_32(cq_head_doorbell_offset, admin_queue.completion_head as u32);

    cqe
}

/// 轮询 I/O CQ 的一个 CQE，并维护 CQ head / phase / CQ head doorbell。
///
/// 与 Admin CQ 逻辑完全一样，只是 queue id 是 I/O queue 的 QID。
/// Linux 5.4.29 参考：
/// - `drivers/nvme/host/pci.c:924-928`：用 phase tag 判断 CQE 是否有效；
/// - `drivers/nvme/host/pci.c:986-993`：CQ head wrap 时翻转 phase；
/// - `drivers/nvme/host/pci.c:1009-1010`：消费 CQE 后敲 CQ head doorbell。
unsafe fn poll_io_cqe(
    bar0_space: &BarSpace,
    io_queue: &mut NvmeQueueState,
    operation_name: &str,
) -> NvmeCompletionQueueEntry {
    let completion_index = io_queue.completion_head as usize;
    let expected_phase = io_queue.completion_phase as u16;
    let target = (io_queue.completion_queue_dma_base.0 as *const NvmeCompletionQueueEntry)
        .add(completion_index);
    let mut cqe = unsafe { read_volatile(target) };
    let mut spin_count = 0usize;

    while (cqe.status & 1) != expected_phase {
        spin_count += 1;
        if spin_count % 1_000_000 == 0 {
            debug!(
                "wait {} io cqe[{}]: spin={} status={:#x} sq_id={} cid={}",
                operation_name,
                completion_index,
                spin_count,
                cqe.status,
                cqe.submission_queue_id,
                cqe.command_id
            );
        }
        cqe = unsafe { read_volatile(target) };
    }

    let nvme_status = cqe.status >> 1;
    if nvme_status == 0 {
        // info!("NVMe IO {} success: cqe={:?}", operation_name, cqe);
    } else {
        error!(
            "NVMe IO {} failed: cqe={:?} nvme_status={:#x}",
            operation_name, cqe, nvme_status
        );
    }

    io_queue.completion_head += 1;
    if io_queue.completion_head == io_queue.queue_depth {
        io_queue.completion_head = 0;
        io_queue.completion_phase ^= 1;
    }

    let cq_head_doorbell_offset = io_queue
        .doorbell_stride
        .completion_head_offset(io_queue.queue_id, io_queue.doorbell_base_offset);
    bar0_space.write_32(cq_head_doorbell_offset, io_queue.completion_head as u32);

    cqe
}

/// 提交一个 64B I/O SQE，并轮询等待对应 CQE。
///
/// 注意 doorbell 写的是“新的 SQ tail index”，不是命令数量。
/// QID=1 且 DSTRD=0 时，SQ1 tail doorbell 是 `0x1008`。
unsafe fn submit_io_sqe_polling<Command>(
    bar0_space: &BarSpace,
    io_queue: &mut NvmeQueueState,
    command: &Command,
    operation_name: &str,
) -> NvmeCompletionQueueEntry {
    let sq_slot = io_queue.submission_tail as usize;
    unsafe {
        write_queue_sqe(io_queue.submission_queue_dma_base, sq_slot, command);
        asm!("fence iorw, iorw");
    }

    io_queue.submission_tail += 1;
    if io_queue.submission_tail == io_queue.queue_depth {
        io_queue.submission_tail = 0;
    }

    let sq_tail_doorbell_offset = io_queue
        .doorbell_stride
        .submission_tail_offset(io_queue.queue_id, io_queue.doorbell_base_offset);
    bar0_space.write_32(sq_tail_doorbell_offset, io_queue.submission_tail as u32);

    unsafe { poll_io_cqe(bar0_space, io_queue, operation_name) }
}

/// Completion Queue Entry。
///
/// 参考 Linux 5.4.29:
/// - `include/linux/nvme.h:1377-1390`
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct NvmeCompletionQueueEntry {
    /// 命令返回结果；第一版可以先只把它当 `u64` 看。
    pub result: u64,
    /// 设备已经消费到的 SQ head。
    pub submission_queue_head: u16,
    /// 完成项来自哪个 SQ。
    pub submission_queue_id: u16,
    /// 完成的是哪个 CID。
    pub command_id: u16,
    /// status 的 bit0 是 phase tag；其余位编码状态码。
    pub status: u16,
}

/// 4096B Identify Controller 数据缓冲。
///
/// TODO(dirinkbottle):
/// 后续实现完整控制器能力解析：
/// 1. `mdts`：最大传输大小；
/// 2. `oacs/oncs`：admin/NVM 可选能力；
/// 3. power state、firmware slot、queue feature 等高级字段。
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct NvmeIdentifyControllerData {
    pub raw_bytes: [u8; 4096],
}

impl NvmeIdentifyControllerData {
    pub const fn zeroed() -> Self {
        Self {
            raw_bytes: [0; 4096],
        }
    }

    /// 从 Identify Controller 数据里取 namespace 数量 `NN`。
    ///
    /// 参考 Linux 5.4.29:
    /// - `include/linux/nvme.h:244`：`struct nvme_id_ctrl` 中 `nn` 字段。
    pub fn namespace_count(&self) -> u32 {
        u32::from_le_bytes([
            self.raw_bytes[516],
            self.raw_bytes[517],
            self.raw_bytes[518],
            self.raw_bytes[519],
        ])
    }

    /// 打印第一版最需要确认的控制器信息。
    pub fn log_summary(&self) {
        let vid = u16::from_le_bytes([self.raw_bytes[0], self.raw_bytes[1]]);
        let ssvid = u16::from_le_bytes([self.raw_bytes[2], self.raw_bytes[3]]);
        let serial_number = core::str::from_utf8(&self.raw_bytes[4..24])
            .unwrap_or("<bad-sn>")
            .trim();
        let model_number = core::str::from_utf8(&self.raw_bytes[24..64])
            .unwrap_or("<bad-mn>")
            .trim();
        let firmware_revision = core::str::from_utf8(&self.raw_bytes[64..72])
            .unwrap_or("<bad-fr>")
            .trim();

        info!(
            "NVMe Identify Controller: vid={:#x} ssvid={:#x} nn={} sn={} mn={} fr={}",
            vid,
            ssvid,
            self.namespace_count(),
            serial_number,
            model_number,
            firmware_revision,
        );
    }
}

/// 4096B Identify Namespace 数据缓冲。
///
/// TODO(dirinkbottle):
/// 后续实现完整 namespace 能力解析：
/// 1. metadata / protection information；
/// 2. thin provisioning / deallocation 能力；
/// 3. 多 LBA format 的选择和格式化支持。
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct NvmeIdentifyNamespaceData {
    pub raw_bytes: [u8; 4096],
}

impl NvmeIdentifyNamespaceData {
    pub const fn zeroed() -> Self {
        Self {
            raw_bytes: [0; 4096],
        }
    }

    /// 解析 Namespace 总逻辑块数 `NSZE`。
    ///
    /// 参考 Linux 5.4.29:
    /// - `include/linux/nvme.h:300-354`：`struct nvme_id_ns`。
    pub fn namespace_size(&self) -> u64 {
        u64::from_le_bytes([
            self.raw_bytes[0],
            self.raw_bytes[1],
            self.raw_bytes[2],
            self.raw_bytes[3],
            self.raw_bytes[4],
            self.raw_bytes[5],
            self.raw_bytes[6],
            self.raw_bytes[7],
        ])
    }

    /// 解析当前 active LBA format 对应的逻辑块大小。
    ///
    /// 流程：
    /// 1. 从 `FLBAS` 低 4 位取 active LBAF 下标；
    /// 2. 找到 `LBAF[active].ds`，它表示 logical block size = `1 << ds`；
    /// 3. 和 Linux 一样，`ds == 0` 时兜底为 512B，避免坏数据导致 1B block。
    ///
    /// 参考 Linux 5.4.29:
    /// - `include/linux/nvme.h:300-341`：Identify Namespace 中 `flbas` 与 `lbaf[16]` 布局；
    /// - `drivers/nvme/host/core.c:1830-1832`：读取 `lbaf[flbas & mask].ds`，0 时设为 9。
    pub fn logical_block_size(&self) -> u32 {
        let flbas = self.raw_bytes[26];
        let active_lba_format_index = (flbas & NVME_NS_FLBAS_LBA_MASK) as usize;
        let lbaf_offset = 128 + active_lba_format_index * 4;
        let raw_lba_data_size_shift = self.raw_bytes[lbaf_offset + 2];
        let lba_data_size_shift = if raw_lba_data_size_shift == 0 {
            NVME_DEFAULT_LBA_SHIFT
        } else {
            raw_lba_data_size_shift
        };

        1u32 << lba_data_size_shift
    }

    /// 打印第一版最需要确认的 namespace 信息。
    pub fn log_summary(&self, namespace_id: NvmeNamespaceId) {
        let nsze = self.namespace_size();
        let ncap = u64::from_le_bytes([
            self.raw_bytes[8],
            self.raw_bytes[9],
            self.raw_bytes[10],
            self.raw_bytes[11],
            self.raw_bytes[12],
            self.raw_bytes[13],
            self.raw_bytes[14],
            self.raw_bytes[15],
        ]);
        let nuse = u64::from_le_bytes([
            self.raw_bytes[16],
            self.raw_bytes[17],
            self.raw_bytes[18],
            self.raw_bytes[19],
            self.raw_bytes[20],
            self.raw_bytes[21],
            self.raw_bytes[22],
            self.raw_bytes[23],
        ]);
        let nsfeat = self.raw_bytes[24];
        let nlbaf = self.raw_bytes[25];
        let flbas = self.raw_bytes[26];
        let active_lba_format_index = (flbas & NVME_NS_FLBAS_LBA_MASK) as usize;
        let lbaf_offset = 128 + active_lba_format_index * 4;
        let metadata_size =
            u16::from_le_bytes([self.raw_bytes[lbaf_offset], self.raw_bytes[lbaf_offset + 1]]);
        let raw_lba_data_size_shift = self.raw_bytes[lbaf_offset + 2];
        let lba_data_size_shift = if raw_lba_data_size_shift == 0 {
            NVME_DEFAULT_LBA_SHIFT
        } else {
            raw_lba_data_size_shift
        };
        let relative_performance = self.raw_bytes[lbaf_offset + 3];

        info!(
            "NVMe Identify Namespace: nsid={} nsze={} ncap={} nuse={} nsfeat={:#x} nlbaf={} flbas={:#x} active_lbaf={} ms={} raw_ds={} ds={} block_size={} rp={:#x}",
            namespace_id.0,
            nsze,
            ncap,
            nuse,
            nsfeat,
            nlbaf,
            flbas,
            active_lba_format_index,
            metadata_size,
            raw_lba_data_size_shift,
            lba_data_size_shift,
            self.logical_block_size(),
            relative_performance,
        );
    }
}

/// 一对 SQ/CQ 的最小运行时状态。
///
/// 第一版轮询模式必须真正维护下面这些字段，否则 doorbell 和 completion phase 会错。
#[derive(Clone, Copy, Debug, Default)]
pub struct NvmeQueueState {
    pub queue_id: NvmeQueueId,
    pub queue_depth: u16,
    pub submission_queue_dma_base: PhysiAddr,
    pub completion_queue_dma_base: PhysiAddr,
    pub submission_tail: u16,
    pub completion_head: u16,
    pub completion_phase: u8,
    pub doorbell_base_offset: u32,
    pub doorbell_stride: NvmeDoorbellStride,
}

/// BlueStarOS 里的 NVMe 控制器对象。
///
/// 这不是 Linux `struct nvme_dev` 的一比一翻译，而是当前内核第一版最小可用集合。
/// 先把“能读写一个 namespace 并挂到 VFS”做通，再考虑 PRP pool、MSI-X、多队列。
#[derive(Debug)]
pub struct NvmeController {
    /// PCIe 枚举结果快照。
    pub pcie_device: PcieDeviceInfo,
    /// 控制器当前使用的 namespace。
    pub namespace_id: NvmeNamespaceId,
    /// 逻辑块大小（字节）。
    pub logical_block_size: u32,
    /// namespace 总逻辑块数。
    pub total_logical_blocks: u64,
    /// doorbell register 步进。
    pub doorbell_stride: NvmeDoorbellStride,
    /// Admin Queue 状态。
    pub admin_queue: NvmeQueueState,
    /// 第一版唯一的 I/O Queue。
    pub io_queue: NvmeQueueState,
    /// Admin SQ 对应的物理页帧所有权。
    ///
    /// 这些页会被控制器通过 ASQ DMA 访问，因此必须和控制器对象同生命周期。
    /// 之前这里用 `mem::forget()` 强行泄漏；现在改成由控制器持有，驱动释放时自动回收。
    pub admin_submission_queue_frames: Vec<FramTracker>,
    /// Admin CQ 对应的物理页帧所有权。
    pub admin_completion_queue_frames: Vec<FramTracker>,
    /// 第一版唯一 I/O SQ 的物理页帧所有权。
    pub io_submission_queue_frames: Vec<FramTracker>,
    /// 第一版唯一 I/O CQ 的物理页帧所有权。
    pub io_completion_queue_frames: Vec<FramTracker>,
}

/// 暴露给 VFS 的 NVMe 块设备。
///
/// VFS 的 `BlockDevTrait` 以 512B sector 为基本单位，但 NVMe namespace 的
/// logical block size 不能假设，必须来自 Identify Namespace 的 active LBAF。
///
/// 所以这里做一层适配：
/// - NVMe block == 512B：VFS sector 与 NVMe LBA 一一映射；
/// - NVMe block > 512B：一个 VFS sector 落在某个 NVMe block 内，写入要 RMW；
/// - NVMe block < 512B：一个 VFS sector 覆盖多个 NVMe blocks。
///
/// TODO(dirinkbottle):
/// 长期最好让 VFS / block layer 也能表达真实 logical block size，而不是永远
/// 对上层伪装成 512B sector。
pub struct NvmeBlockDevice {
    controller: NvmeController,
}

unsafe impl Send for NvmeBlockDevice {}
unsafe impl Sync for NvmeBlockDevice {}

/// 将 NVMe 内部使用的 Linux errno 风格错误转换成 VFS 层错误。
///
/// 流程：
/// 1. 参数错误、设备不存在、能力不支持等“可分类错误”保留语义；
/// 2. 控制器命令失败、队列状态异常等底层失败统一归为 `IO`；
/// 3. 这样 VFS 上层不用理解 NVMe 细节，但不会把所有错误都误判成磁盘 I/O。
fn nvme_blue_err_to_vfs_error(error: BlueErr) -> VfsFsError {
    match error {
        BlueErr::EINVAL => VfsFsError::Invalid,
        BlueErr::ENODEV | BlueErr::ENXIO => VfsFsError::NoDevice,
        BlueErr::EOPNOTSUPP | BlueErr::ENOSYS => VfsFsError::NotSupported,
        BlueErr::ENOMEM | BlueErr::ENOSPC | BlueErr::ENOBUFS => VfsFsError::NoSpace,
        BlueErr::EBUSY => VfsFsError::Busy,
        BlueErr::EACCES | BlueErr::EPERM => VfsFsError::PermissionDenied,
        BlueErr::EPIPE => VfsFsError::BrokenPipe,
        _ => VfsFsError::IO,
    }
}

/// 将 VFS 层块设备注册错误转换成 NVMe probe 对外返回的 Linux errno 风格错误。
///
/// NVMe probe 函数返回 `BlueErr`，VFS 块设备构造返回 `VfsFsError`，这里集中桥接，
/// 避免调用点随手 `.map_err(|_| EIO)` 把错误语义抹掉。
fn vfs_error_to_nvme_blue_err(error: VfsFsError) -> BlueErr {
    match error {
        VfsFsError::Invalid => BlueErr::EINVAL,
        VfsFsError::NoDevice => BlueErr::ENODEV,
        VfsFsError::NotSupported => BlueErr::EOPNOTSUPP,
        VfsFsError::NoSpace => BlueErr::ENOMEM,
        VfsFsError::Busy => BlueErr::EBUSY,
        VfsFsError::PermissionDenied => BlueErr::EACCES,
        VfsFsError::BrokenPipe => BlueErr::EPIPE,
        _ => BlueErr::EIO,
    }
}

impl NvmeBlockDevice {
    /// 用已经完成 bring-up 的控制器构造 VFS 块设备。
    pub fn new(controller: NvmeController) -> Result<Self, VfsFsError> {
        let logical_block_size = controller.logical_block_size as usize;
        if logical_block_size == 0 {
            error!(
                "NVMe logical block size is invalid: block_size={}",
                controller.logical_block_size
            );
            return Err(VfsFsError::Invalid);
        }
        if logical_block_size > PAGE_SIZE {
            error!(
                "NVMe logical block size is unsupported by first PRP-only path: block_size={} page_size={}",
                logical_block_size, PAGE_SIZE
            );
            return Err(VfsFsError::NotSupported);
        }
        if logical_block_size >= SECTOR_SIZE && logical_block_size % SECTOR_SIZE != 0 {
            error!(
                "NVMe logical block size is not a multiple of VFS sector size: block_size={} sector_size={}",
                logical_block_size, SECTOR_SIZE
            );
            return Err(VfsFsError::NotSupported);
        }
        if logical_block_size < SECTOR_SIZE && SECTOR_SIZE % logical_block_size != 0 {
            error!(
                "VFS sector size is not a multiple of NVMe logical block size: block_size={} sector_size={}",
                logical_block_size, SECTOR_SIZE
            );
            return Err(VfsFsError::NotSupported);
        }

        Ok(Self { controller })
    }

    /// 当前 namespace 的实际逻辑块大小。
    fn logical_block_size(&self) -> usize {
        self.controller.logical_block_size as usize
    }

    /// 将 VFS 的 512B sector LBA 映射到 NVMe namespace 的实际 LBA。
    ///
    /// Linux 5.4.29 参考：
    /// - `drivers/nvme/host/core.c:1759`：capacity 用 `nsze << (lba_shift - 9)` 换算成 512B sector；
    /// - `drivers/nvme/host/core.c:1792`、`:3501`：把 namespace LBA 大小设置为 block queue logical block size；
    /// - `block/partitions/efi.c:242`：GPT 解析也会按 `logical_block_size / 512` 做 LBA 换算。
    ///
    /// 返回值：
    /// 1. NVMe 起始 LBA；
    /// 2. 512B sector 在第一个 NVMe logical block 内的字节偏移；
    /// 3. 需要读写的 NVMe logical block 数量。
    fn map_vfs_sector_to_nvme_range(
        &self,
        vfs_lba: usize,
    ) -> Result<(u64, usize, u16), VfsFsError> {
        if vfs_lba as u64 >= self.capacity_in_sectors() {
            return Err(VfsFsError::Invalid);
        }

        let logical_block_size = self.logical_block_size();
        let byte_offset = (vfs_lba as u128) * (SECTOR_SIZE as u128);
        let nvme_lba = (byte_offset / logical_block_size as u128) as u64;
        let offset_in_nvme_block = (byte_offset % logical_block_size as u128) as usize;
        let nvme_block_count = if logical_block_size >= SECTOR_SIZE {
            1
        } else {
            (SECTOR_SIZE / logical_block_size) as u16
        };

        Ok((nvme_lba, offset_in_nvme_block, nvme_block_count))
    }
}

impl BlockDevTrait for NvmeBlockDevice {
    /// 从 NVMe namespace 读取一个 VFS sector。
    ///
    /// 流程：
    /// 1. 分配一页 DMA 缓冲；
    /// 2. 根据 Identify Namespace 得到的实际 logical block size 计算 NVMe LBA；
    /// 3. 发 NVMe Read，PRP1 指向该 DMA 页；
    /// 4. 从实际 logical block 的对应偏移处拷贝 512B 给 VFS。
    fn read_block(&mut self, lba: usize, buf: &mut [u8]) -> Result<(), VfsFsError> {
        if buf.len() < SECTOR_SIZE {
            return Err(VfsFsError::Invalid);
        }

        let (nvme_lba, offset_in_nvme_block, nvme_block_count) =
            self.map_vfs_sector_to_nvme_range(lba)?;
        let dma_frames = alloc_contiguous_frames(1).ok_or(VfsFsError::NoSpace)?;
        let dma_addr: PhysiAddr = dma_frames[0].ppn.into();
        self.controller
            .read_logical_blocks_polling(nvme_lba, nvme_block_count, dma_addr)
            .map_err(nvme_blue_err_to_vfs_error)?;

        unsafe {
            core::ptr::copy_nonoverlapping(
                (dma_addr.0 + offset_in_nvme_block) as *const u8,
                buf.as_mut_ptr(),
                SECTOR_SIZE,
            );
        }

        Ok(())
    }

    /// 向 NVMe namespace 写入一个 VFS sector。
    ///
    /// 流程：
    /// 1. 分配一页 DMA 缓冲；
    /// 2. 如果 NVMe logical block 大于 512B，先读整块做 read-modify-write；
    /// 3. 把 VFS 传入的 512B 写入实际 logical block 内对应偏移；
    /// 4. 发 NVMe Write。
    fn write_block(&mut self, lba: usize, buf: &[u8]) -> Result<(), VfsFsError> {
        if buf.len() < SECTOR_SIZE {
            return Err(VfsFsError::Invalid);
        }

        let (nvme_lba, offset_in_nvme_block, nvme_block_count) =
            self.map_vfs_sector_to_nvme_range(lba)?;
        let dma_frames = alloc_contiguous_frames(1).ok_or(VfsFsError::NoSpace)?;
        let dma_addr: PhysiAddr = dma_frames[0].ppn.into();

        if self.logical_block_size() > SECTOR_SIZE {
            self.controller
                .read_logical_blocks_polling(nvme_lba, nvme_block_count, dma_addr)
                .map_err(nvme_blue_err_to_vfs_error)?;
        }

        unsafe {
            core::ptr::copy_nonoverlapping(
                buf.as_ptr(),
                (dma_addr.0 + offset_in_nvme_block) as *mut u8,
                SECTOR_SIZE,
            );
            asm!("fence iorw, iorw");
        }

        self.controller
            .write_logical_blocks_polling(nvme_lba, nvme_block_count, dma_addr)
            .map_err(nvme_blue_err_to_vfs_error)
    }

    /// 将 NVMe volatile write cache 中的数据刷到非易失介质。
    fn flush(&mut self) -> Result<(), VfsFsError> {
        self.controller
            .flush_namespace_polling()
            .map_err(nvme_blue_err_to_vfs_error)
    }

    /// 返回 namespace 中的 512B sector 数量。
    fn capacity_in_sectors(&self) -> u64 {
        let total_bytes =
            (self.controller.total_logical_blocks as u128) * (self.logical_block_size() as u128);
        (total_bytes / SECTOR_SIZE as u128) as u64
    }
}

impl NvmeController {
    /// 从 PCIe 设备描述里重新建立 BAR0 MMIO 视图。
    ///
    /// `BarSpace` 本身只是一个 `(start, end)` 轻量视图，不拥有资源；控制器对象保存
    /// `PcieDeviceInfo` 快照，每次需要访问寄存器时按 BAR 信息重建即可。
    fn build_bar0_space_from_device(device: &PcieDeviceInfo) -> Result<BarSpace, BlueErr> {
        if device.class_code != NVME_PCI_CLASS_CODE {
            error!(
                "PCIe device class is not NVMe: class={:#x}",
                device.class_code
            );
            return Err(BlueErr::EINVAL);
        }

        device
            .bars
            .iter()
            .find(|bar| {
                bar.bar_index == NVME_CONTROLLER_BAR_INDEX && bar.space == PcieBarSpace::Memory64
            })
            .map(|bar| bar.build_bar_space())
            .ok_or(BlueErr::ENODEV)
    }

    /// 获取当前控制器 BAR0 MMIO 视图。
    fn bar0_space(&self) -> Result<BarSpace, BlueErr> {
        Self::build_bar0_space_from_device(&self.pcie_device)
    }

    /// 打开 PCI Command 寄存器里的 Memory Space 与 Bus Master。
    ///
    /// NVMe controller 的寄存器和队列 DMA 都依赖这两个位：
    /// 1. `PCI_COMMAND_MEMORY` 允许 CPU 访问 BAR0 MMIO；
    /// 2. `PCI_COMMAND_MASTER` 允许设备主动 DMA 读写内存。
    fn enable_pci_memory_and_bus_master(device: &PcieDeviceInfo) {
        let bdf = device.bdf();
        let old_command = unsafe { cfg_read16(bdf.0, bdf.1, bdf.2, PCI_COMMAND) };
        let new_command = old_command | PCI_COMMAND_MEMORY | PCI_COMMAND_MASTER;
        unsafe { cfg_write16(bdf.0, bdf.1, bdf.2, PCI_COMMAND, new_command) };
    }

    /// 等待 `CSTS.RDY` 到达目标状态。
    ///
    /// 参考 Linux 5.4.29:
    /// - `drivers/nvme/host/core.c:2060-2084`：`nvme_wait_ready()`。
    fn wait_ready(
        bar0_space: &BarSpace,
        expected_ready: bool,
        operation_name: &str,
    ) -> Result<(), BlueErr> {
        let expected_bit = if expected_ready { csts::RDY } else { 0 };

        // TODO(dirinkbottle): 后面应该按 CAP.TO 计算超时，当前先沿用第一版固定轮询。
        for _ in 0..100 {
            let controller_status = bar0_space.read_32(NVME_REG_CSTS);
            if controller_status == u32::MAX {
                error!("NVMe disappeared while waiting {}", operation_name);
                return Err(BlueErr::EIO);
            }
            if (controller_status & csts::RDY) == expected_bit {
                return Ok(());
            }
            if (controller_status & csts::CFS) != 0 {
                error!(
                    "NVMe fatal controller status while waiting {}: csts={:#x}",
                    operation_name, controller_status
                );
                return Err(BlueErr::EIO);
            }
            kernel_sleep(10);
        }

        error!("NVMe wait {} timeout", operation_name);
        Err(BlueErr::EIO)
    }

    /// 初始化 Admin SQ/CQ 并写入 `AQA/ASQ/ACQ`。
    ///
    /// 参考 Linux 5.4.29:
    /// - `drivers/nvme/host/pci.c:1697-1711`：disable 后分配 Admin Queue 并写 AQA/ASQ/ACQ。
    fn setup_admin_queue(&mut self, capability: u64) -> Result<(), BlueErr> {
        let bar0_space = self.bar0_space()?;
        let admin_sq_frames = alloc_contiguous_frames(1).ok_or(BlueErr::ENOMEM)?;
        let admin_cq_frames = alloc_contiguous_frames(1).ok_or(BlueErr::ENOMEM)?;
        let admin_sq_dma_addr: PhysiAddr = admin_sq_frames[0].ppn.into();
        let admin_cq_dma_addr: PhysiAddr = admin_cq_frames[0].ppn.into();
        let max_queue_entries_zero_based = (capability & 0xffff) as u16;
        let admin_queue_depth_zero_based =
            max_queue_entries_zero_based.min(NVME_ADMIN_QUEUE_DEPTH - 1);

        self.admin_queue = NvmeQueueState {
            queue_id: NvmeQueueId::ADMIN,
            queue_depth: admin_queue_depth_zero_based + 1,
            submission_queue_dma_base: admin_sq_dma_addr,
            completion_queue_dma_base: admin_cq_dma_addr,
            submission_tail: 0,
            completion_head: 0,
            completion_phase: 1,
            doorbell_base_offset: NVME_REG_DOORBELL_BASE as u32,
            doorbell_stride: self.doorbell_stride,
        };

        let admin_queue_attributes = ((admin_queue_depth_zero_based & 0x0fff) as u32) << 16
            | (admin_queue_depth_zero_based & 0x0fff) as u32;
        bar0_space.write_32(NVME_REG_AQA, admin_queue_attributes);
        bar0_space.write_64(NVME_REG_ASQ, admin_sq_dma_addr.0 as u64);
        bar0_space.write_64(NVME_REG_ACQ, admin_cq_dma_addr.0 as u64);

        debug!("Build admin queue: {:?}", self.admin_queue);

        // 把队列 backing pages 的所有权挂到控制器对象上，避免 `mem::forget()`
        // 导致永久泄漏，同时保持 DMA 生命周期覆盖整个控制器生命周期。
        self.admin_submission_queue_frames = admin_sq_frames;
        self.admin_completion_queue_frames = admin_cq_frames;

        Ok(())
    }

    /// 从一个已经完成 PCIe 枚举的 NVMe 控制器构造驱动对象。
    ///
    /// 流程按 Linux 的控制器 bring-up 顺序收束到本模块的框架函数：
    /// 1. 校验 class code / BAR0；
    /// 2. 打开 `PCI_COMMAND_MEMORY | PCI_COMMAND_MASTER`；
    /// 3. 如果 `CC.EN=1`，先 disable；
    /// 4. 分配并注册 Admin Queue；
    /// 5. enable controller；
    /// 6. Identify Controller / Namespace；
    /// 7. 创建第一对 I/O Queue。
    ///
    /// Linux 参考：
    /// - `drivers/nvme/host/core.c:2060-2147`
    /// - `drivers/nvme/host/pci.c:1690-1711`
    /// - `drivers/nvme/host/pci.c:1116-1167`
    pub fn probe_from_pcie_device(pcie_device: PcieDeviceInfo) -> Result<Self, BlueErr> {
        let bar0_space = Self::build_bar0_space_from_device(&pcie_device)?;
        Self::enable_pci_memory_and_bus_master(&pcie_device);

        let capability = bar0_space.read_64(NVME_REG_CAP);
        let version = bar0_space.read_32(NVME_REG_VS);
        let controller_status = bar0_space.read_32(NVME_REG_CSTS);
        let controller_config = bar0_space.read_32(NVME_REG_CC);
        let doorbell_stride = NvmeDoorbellStride::from_capability(capability);

        debug!(
            "Already probe nvme device {:?} cap={:#x} vs={:#x} csts={:#x} cc={:#x}",
            pcie_device.bdf(),
            capability,
            version,
            controller_status,
            controller_config,
        );
        debug!("NVMe doorbell stride: {} bytes", doorbell_stride.bytes);

        let mut controller = Self {
            pcie_device,
            namespace_id: NvmeNamespaceId::PRIMARY,
            logical_block_size: 0,
            total_logical_blocks: 0,
            doorbell_stride,
            admin_queue: NvmeQueueState::default(),
            io_queue: NvmeQueueState::default(),
            admin_submission_queue_frames: Vec::new(),
            admin_completion_queue_frames: Vec::new(),
            io_submission_queue_frames: Vec::new(),
            io_completion_queue_frames: Vec::new(),
        };

        if (controller_config & cc::ENABLE) != 0 {
            controller.disable_controller()?;
        }
        controller.setup_admin_queue(capability)?;
        controller.enable_controller()?;

        let identify_controller_data = controller.identify_controller()?;
        identify_controller_data.log_summary();
        if identify_controller_data.namespace_count() == 0 {
            error!("NVMe controller reports zero namespace");
            return Err(BlueErr::ENODEV);
        }

        let namespace_id = NvmeNamespaceId::PRIMARY;
        let identify_namespace_data = controller.identify_namespace(namespace_id)?;
        identify_namespace_data.log_summary(namespace_id);
        controller.namespace_id = namespace_id;
        controller.logical_block_size = identify_namespace_data.logical_block_size();
        controller.total_logical_blocks = identify_namespace_data.namespace_size();

        controller.create_first_io_queue_pair()?;

        Ok(controller)
    }

    /// 控制器 disable 流程。
    ///
    /// 关键点：
    /// 1. 清 `CC.EN`；
    /// 2. 轮询 `CSTS.RDY == 0`；
    /// 3. 如果 `CSTS == 0xffff_ffff`，按设备消失处理。
    ///
    /// 参考 Linux 5.4.29:
    /// - `drivers/nvme/host/core.c:2093-2108`
    pub fn disable_controller(&mut self) -> Result<(), BlueErr> {
        let bar0_space = self.bar0_space()?;
        let controller_config = bar0_space.read_32(NVME_REG_CC);

        if (controller_config & cc::ENABLE) == 0 {
            return Ok(());
        }

        // Linux 会同时清 SHN 与 EN。当前还没封装 SHN 位，第一版先只清 EN。
        bar0_space.write_32(NVME_REG_CC, controller_config & !cc::ENABLE);
        Self::wait_ready(&bar0_space, false, "disable-controller")
    }

    /// 控制器 enable 流程。
    ///
    /// 关键点：
    /// 1. 读取 `CAP`；
    /// 2. 检查 `CAP.MPSMIN`；
    /// 3. 构造 `CC`；
    /// 4. 置位 `CC.EN`；
    /// 5. 轮询 `CSTS.RDY == 1`。
    ///
    /// 参考 Linux 5.4.29:
    /// - `drivers/nvme/host/core.c:2111-2147`
    pub fn enable_controller(&mut self) -> Result<(), BlueErr> {
        let bar0_space = self.bar0_space()?;
        let capability = bar0_space.read_64(NVME_REG_CAP);
        let min_page_shift = (((capability >> 48) & 0x0f) as u32) + 12;

        if min_page_shift > 12 {
            error!(
                "NVMe minimum page size is too large: min_page_shift={}",
                min_page_shift
            );
            return Err(BlueErr::EOPNOTSUPP);
        }

        self.doorbell_stride = NvmeDoorbellStride::from_capability(capability);
        self.admin_queue.doorbell_stride = self.doorbell_stride;
        self.io_queue.doorbell_stride = self.doorbell_stride;

        // Linux 5.4.29 `drivers/nvme/host/core.c:2137-2141`：
        // CSS=NVM, MPS=4KiB, AMS=RoundRobin, SQE=64B, CQE=16B, EN=1。
        let controller_config = cc::CSS_NVM
            | cc::AMS_ROUND_ROBIN
            | (0 << cc::MPS_SHIFT)
            | (NVME_CC_IOSQES_64B << cc::IOSQES_SHIFT)
            | (NVME_CC_IOCQES_16B << cc::IOCQES_SHIFT)
            | cc::ENABLE;
        bar0_space.write_32(NVME_REG_CC, controller_config);
        Self::wait_ready(&bar0_space, true, "enable-controller")
    }

    /// 发一个 identify controller 命令。
    pub fn identify_controller(&mut self) -> Result<NvmeIdentifyControllerData, BlueErr> {
        let identify_frames = alloc_contiguous_frames(1).ok_or(BlueErr::ENOMEM)?;
        let identify_dma_addr: PhysiAddr = identify_frames[0].ppn.into();
        let identify_command = NvmeIdentifyCommand {
            opcode: NvmeAdminOpcode::Identify as u8,
            command_id: next_command_id().0,
            namespace_id: 0,
            data_pointer: NvmePrpDataPointer {
                prp1: identify_dma_addr.0 as u64,
                prp2: 0,
            },
            controller_or_namespace_structure: 0x01,
            ..Default::default()
        };

        let cqe = self.submit_admin_sqe_polling(&identify_command, "identify-controller")?;
        if (cqe.status >> 1) != 0 {
            return Err(BlueErr::EIO);
        }

        let mut identify_data = NvmeIdentifyControllerData::zeroed();
        unsafe {
            asm!("fence iorw, iorw");
            core::ptr::copy_nonoverlapping(
                identify_dma_addr.0 as *const u8,
                identify_data.raw_bytes.as_mut_ptr(),
                identify_data.raw_bytes.len(),
            );
        }

        Ok(identify_data)
    }

    /// 发一个 identify namespace 命令。
    pub fn identify_namespace(
        &mut self,
        namespace_id: NvmeNamespaceId,
    ) -> Result<NvmeIdentifyNamespaceData, BlueErr> {
        let identify_frames = alloc_contiguous_frames(1).ok_or(BlueErr::ENOMEM)?;
        let identify_dma_addr: PhysiAddr = identify_frames[0].ppn.into();
        let identify_command = NvmeIdentifyCommand {
            opcode: NvmeAdminOpcode::Identify as u8,
            command_id: next_command_id().0,
            namespace_id: namespace_id.0,
            data_pointer: NvmePrpDataPointer {
                prp1: identify_dma_addr.0 as u64,
                prp2: 0,
            },
            controller_or_namespace_structure: 0x00,
            ..Default::default()
        };

        let cqe = self.submit_admin_sqe_polling(&identify_command, "identify-namespace")?;
        if (cqe.status >> 1) != 0 {
            return Err(BlueErr::EIO);
        }

        let mut identify_data = NvmeIdentifyNamespaceData::zeroed();
        unsafe {
            asm!("fence iorw, iorw");
            core::ptr::copy_nonoverlapping(
                identify_dma_addr.0 as *const u8,
                identify_data.raw_bytes.as_mut_ptr(),
                identify_data.raw_bytes.len(),
            );
        }

        Ok(identify_data)
    }

    /// 创建第一对 I/O CQ/SQ。
    ///
    /// 参考 Linux 5.4.29:
    /// - `drivers/nvme/host/pci.c:1116-1167`
    /// - `drivers/nvme/host/pci.c:1729-1764`
    pub fn create_first_io_queue_pair(&mut self) -> Result<(), BlueErr> {
        let io_sq_frames = alloc_contiguous_frames(1).ok_or(BlueErr::ENOMEM)?;
        let io_cq_frames = alloc_contiguous_frames(1).ok_or(BlueErr::ENOMEM)?;
        let io_sq_dma_addr: PhysiAddr = io_sq_frames[0].ppn.into();
        let io_cq_dma_addr: PhysiAddr = io_cq_frames[0].ppn.into();
        let queue_id = NvmeQueueId::IO0;
        let queue_depth = NVME_IO_QUEUE_DEPTH;

        let create_cq_command = NvmeCreateCompletionQueueCommand {
            opcode: NvmeAdminOpcode::CreateCompletionQueue as u8,
            command_id: next_command_id().0,
            prp1: io_cq_dma_addr.0 as u64,
            completion_queue_id: queue_id.0,
            queue_size_zero_based: queue_depth - 1,
            // bit0=Physically Contiguous；第一版 polling，不打开 IRQ bit1。
            completion_queue_flags: 0x0001,
            interrupt_vector: 0,
            ..Default::default()
        };
        let create_sq_command = NvmeCreateSubmissionQueueCommand {
            opcode: NvmeAdminOpcode::CreateSubmissionQueue as u8,
            command_id: next_command_id().0,
            prp1: io_sq_dma_addr.0 as u64,
            submission_queue_id: queue_id.0,
            queue_size_zero_based: queue_depth - 1,
            // bit0=Physically Contiguous，priority=0。
            submission_queue_flags: 0x0001,
            completion_queue_id: queue_id.0,
            ..Default::default()
        };

        let create_cq_cqe = self.submit_admin_sqe_polling(&create_cq_command, "create-io-cq")?;
        if (create_cq_cqe.status >> 1) != 0 {
            return Err(BlueErr::EIO);
        }

        let create_sq_cqe = self.submit_admin_sqe_polling(&create_sq_command, "create-io-sq")?;
        if (create_sq_cqe.status >> 1) != 0 {
            return Err(BlueErr::EIO);
        }

        self.io_queue = NvmeQueueState {
            queue_id,
            queue_depth,
            submission_queue_dma_base: io_sq_dma_addr,
            completion_queue_dma_base: io_cq_dma_addr,
            submission_tail: 0,
            completion_head: 0,
            completion_phase: 1,
            doorbell_base_offset: NVME_REG_DOORBELL_BASE as u32,
            doorbell_stride: self.doorbell_stride,
        };

        // 和 admin queue 一样，I/O queue backing pages 由控制器对象持有。
        self.io_submission_queue_frames = io_sq_frames;
        self.io_completion_queue_frames = io_cq_frames;

        Ok(())
    }

    /// 读取若干逻辑块到连续物理缓冲区。
    ///
    /// TODO(dirinkbottle):
    /// 第一版只支持：
    /// 1. 缓冲区物理连续；
    /// 2. 数据长度 <= 4KiB；
    /// 3. 单个 namespace；
    /// 4. PRP1/PRP2 覆盖，不创建 PRP list。
    pub fn read_logical_blocks_polling(
        &mut self,
        start_lba: u64,
        block_count: u16,
        buffer_phys_addr: PhysiAddr,
    ) -> Result<(), BlueErr> {
        self.submit_rw_command_polling(
            NvmeIoOpcode::Read,
            NvmeLogicalBlockAddress(start_lba),
            NvmeLogicalBlockCount(block_count),
            buffer_phys_addr,
            "read-logical-blocks",
        )
    }

    /// 写若干逻辑块到设备。
    pub fn write_logical_blocks_polling(
        &mut self,
        start_lba: u64,
        block_count: u16,
        buffer_phys_addr: PhysiAddr,
    ) -> Result<(), BlueErr> {
        self.submit_rw_command_polling(
            NvmeIoOpcode::Write,
            NvmeLogicalBlockAddress(start_lba),
            NvmeLogicalBlockCount(block_count),
            buffer_phys_addr,
            "write-logical-blocks",
        )
    }

    /// 发送 NVMe Flush 命令，确保 namespace 中已完成的写入落到稳定介质。
    ///
    /// Linux 5.4.29 参考：
    /// - `include/linux/nvme.h:559`：`nvme_cmd_flush = 0x00`；
    /// - `drivers/nvme/host/core.c:600-605`：Flush 命令填写 opcode 与 nsid。
    pub fn flush_namespace_polling(&mut self) -> Result<(), BlueErr> {
        let bar0_space = self.bar0_space()?;
        let command = NvmeRwCommand {
            opcode: NvmeIoOpcode::Flush as u8,
            command_id: next_command_id().0,
            namespace_id: self.namespace_id.0,
            ..Default::default()
        };

        unsafe {
            let cqe =
                submit_io_sqe_polling(&bar0_space, &mut self.io_queue, &command, "flush-namespace");
            if (cqe.status >> 1) != 0 {
                return Err(BlueErr::EIO);
            }
            asm!("fence iorw, iorw");
        }

        Ok(())
    }

    /// 发一个任意 64B Admin SQE，并在 Admin CQ 上轮询完成。
    ///
    /// 这是 `submit_admin_command_polling()` 的泛型内核，原因是 NVMe admin command
    /// 的不同 opcode 在 Rust 里有不同结构体，但硬件看到的都是 64B SQE。
    fn submit_admin_sqe_polling<Command>(
        &mut self,
        command: &Command,
        operation_name: &str,
    ) -> Result<NvmeCompletionQueueEntry, BlueErr> {
        assert!(core::mem::size_of::<Command>() == 64);

        let bar0_space = self.bar0_space()?;
        let submission_slot = self.admin_queue.submission_tail as usize;
        unsafe {
            write_queue_sqe(
                self.admin_queue.submission_queue_dma_base,
                submission_slot,
                command,
            );
            asm!("fence iorw, iorw");
        }

        self.admin_queue.submission_tail += 1;
        if self.admin_queue.submission_tail == self.admin_queue.queue_depth {
            self.admin_queue.submission_tail = 0;
        }

        let sq_tail_doorbell_offset = self.admin_queue.doorbell_stride.submission_tail_offset(
            self.admin_queue.queue_id,
            self.admin_queue.doorbell_base_offset,
        );
        bar0_space.write_32(
            sq_tail_doorbell_offset,
            self.admin_queue.submission_tail as u32,
        );

        let cqe = unsafe { poll_admin_cqe(&bar0_space, &mut self.admin_queue, operation_name) };
        Ok(cqe)
    }

    /// 提交一个 NVMe Read/Write 命令。
    ///
    /// 第一版只支持一个 PRP1 指向的 4KiB 内连续缓冲；跨页和 PRP list 后面补。
    fn submit_rw_command_polling(
        &mut self,
        opcode: NvmeIoOpcode,
        start_lba: NvmeLogicalBlockAddress,
        block_count: NvmeLogicalBlockCount,
        buffer_phys_addr: PhysiAddr,
        operation_name: &str,
    ) -> Result<(), BlueErr> {
        if block_count.0 == 0 {
            return Err(BlueErr::EINVAL);
        }

        let transfer_bytes = block_count.0 as usize * self.logical_block_size as usize;
        if transfer_bytes > PAGE_SIZE {
            // TODO(dirinkbottle): 支持 PRP2 / PRP list 后放开 4KiB 限制。
            error!(
                "NVMe transfer too large for first PRP-only path: bytes={:#x}",
                transfer_bytes
            );
            return Err(BlueErr::EOPNOTSUPP);
        }

        let bar0_space = self.bar0_space()?;
        let command = NvmeRwCommand {
            opcode: opcode as u8,
            command_id: next_command_id().0,
            namespace_id: self.namespace_id.0,
            data_pointer: NvmePrpDataPointer {
                prp1: buffer_phys_addr.0 as u64,
                prp2: 0,
            },
            start_lba: start_lba.0,
            length: block_count.to_zero_based_nlb(),
            ..Default::default()
        };

        unsafe {
            let cqe =
                submit_io_sqe_polling(&bar0_space, &mut self.io_queue, &command, operation_name);
            if (cqe.status >> 1) != 0 {
                return Err(BlueErr::EIO);
            }
            asm!("fence iorw, iorw");
        }

        Ok(())
    }
}

/// 从 `PCIE_DEVICES` 中探测 NVMe 控制器并注册到 VFS 块设备表。
///
/// 这个函数已经由 `driver/pcie/mod.rs:pci_probe_callback()` 在 PCIe 扫描完成后调用。
/// 注册成功后，`RootFs::init_rootfs()` 会遍历 `GLOBAL_BLOCKS` 并自动生成 `/vdX` 节点。
pub fn probe_registered_pcie_nvme_devices() -> Result<(), BlueErr> {
    let pcie_controller = collect_pcie_devices_by_target::<NvmePcieDeviceTarget>();
    let pcie_controller_len = pcie_controller.len();
    if pcie_controller_len == 0 {
        debug!("Empty nvme controller device");
        return Err(BlueErr::ENODEV);
    }
    for nvme_info in pcie_controller {
        match NvmeController::probe_from_pcie_device(nvme_info).and_then(|controller| {
            NvmeBlockDevice::new(controller).map_err(vfs_error_to_nvme_blue_err)
        }) {
            Ok(block_device) => {
                info!(
                    "NVMe block device registered: sectors={}",
                    block_device.capacity_in_sectors()
                );
                register_global_block_device(Arc::new(Mutex::new(block_device)));
            }
            Err(error) => {
                error!("NVMe controller probe failed: {:?}", error);
            }
        }
    }

    Ok(())
}
