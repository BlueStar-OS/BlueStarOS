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
