/// NVMe 控制器寄存器偏移量 (BAR0 映射空间)
pub mod nvme_regs {
    /// Controller Capabilities (CAP) - 8 Bytes (64-bit)
    /// 包含：最大队列深度 (MQES)、门铃步进 (DSTRD)、超时时间 (TO)、页大小限制 (MPS)
    pub const NVME_REG_CAP: usize = 0x00;

    /// Version (VS) - 4 Bytes (32-bit)
    /// 包含：主版本号、次版本号 (例如 1.4.0)
    pub const NVME_REG_VS: usize = 0x08;

    /// Controller Configuration (CC) - 4 Bytes (32-bit)
    /// 重要位：Enable (EN, Bit 0), I/O Completion Queue Entry Size (IOCQES),
    /// I/O Submission Queue Entry Size (IOSQES)
    pub const NVME_REG_CC: usize = 0x14;

    /// Controller Status (CSTS) - 4 Bytes (32-bit)
    /// 重要位：Ready (RDY, Bit 0), Controller Fatal Status (CFS, Bit 1)
    pub const NVME_REG_CSTS: usize = 0x1C;

    /// Admin Queue Attributes (AQA) - 4 Bytes (32-bit)
    /// 包含：Admin SQ Size (Bits 0-11), Admin CQ Size (Bits 16-27)
    /// 注意：写入的值是 Depth - 1
    pub const NVME_REG_AQA: usize = 0x24;

    /// Admin Submission Queue Base Address (ASQ) - 8 Bytes (64-bit)
    /// 必须是 4KB 对齐的物理地址
    pub const NVME_REG_ASQ: usize = 0x28;

    /// Admin Completion Queue Base Address (ACQ) - 8 Bytes (64-bit)
    /// 必须是 4KB 对齐的物理地址
    pub const NVME_REG_ACQ: usize = 0x30;

    /// Doorbell 寄存器起始偏移 - 0x1000
    /// 每个队列的门铃位置取决于 CAP.DSTRD (Doorbell Stride)
    pub const NVME_REG_DOORBELL_BASE: usize = 0x1000;
}

/// CAP 寄存器字段提取 (参考 Linux 5.4.29 include/linux/nvme.h:115-119)
pub mod cap {
    /// 最大队列深度 (MQES): bits [15:0]
    pub fn mqes(cap_val: u64) -> u16 {
        (cap_val & 0xffff) as u16
    }
    /// 超时时间 (TO): bits [23:16]
    pub fn timeout(cap_val: u64) -> u8 {
        ((cap_val >> 24) & 0xff) as u8
    }
    /// 门铃步进 (DSTRD): bits [35:32]
    pub fn stride(cap_val: u64) -> u8 {
        ((cap_val >> 32) & 0x0f) as u8
    }
    /// 最小页大小 (MPSMIN): bits [51:48]
    pub fn mpsmin(cap_val: u64) -> u8 {
        ((cap_val >> 48) & 0x0f) as u8
    }
}

/// AQA 寄存器字段掩码与位移 (参考 Linux 5.4.29 include/linux/nvme.h)
pub mod aqa {
    /// Admin Submission Queue Size 掩码 (bits [11:0])
    pub const ASQS_MASK: u32 = 0x0000_0FFF;
    /// Admin Completion Queue Size 位移 (bits [27:16])
    pub const ACQS_SHIFT: u32 = 16;
}

/// Create I/O Completion/Submission Queue 标志 (参考 Linux 5.4.29 include/linux/nvme.h:849-854)
/// 物理连续队列
pub const NVME_QUEUE_PHYS_CONTIG: u16 = 0x0001;
/// CQ IRQ 使能
pub const NVME_CQ_IRQ_ENABLED: u16 = 0x0002;

/// Identify CNS (Controller or Namespace Structure) 编码 (参考 Linux 5.4.29 include/linux/nvme.h:347-354)
pub mod nvme_identify_cns {
    /// Identify Namespace
    pub const NS: u8 = 0x00;
    /// Identify Controller
    pub const CTRL: u8 = 0x01;
}

/// CQE 状态字段位定义
pub mod cqe_status {
    /// Phase Tag (bit 0): 标记 CQE 是否新到达
    pub const PHASE_TAG: u16 = 0x0001;
    /// 状态码在 status 字段中的位移 (status >> 1 得到 NVMe 状态码)
    pub const STATUS_CODE_SHIFT: u16 = 1;
}
