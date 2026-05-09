/*
 * QEMU/Bochs VESA 扩展 (VBE) 驱动常量
 * 对应 PCI 设备 1234:1111
 */

// --- 硬件限制 (Read Only) ---
/// 硬件支持的最大 X 分辨率 (16000 像素)
pub const VBE_DISPI_MAX_XRES: u16 = 16000;
/// 硬件支持的最大 Y 分辨率 (12000 像素)
pub const VBE_DISPI_MAX_YRES: u16 = 12000;
/// 硬件支持的最大色深 (32 bits per pixel)
pub const VBE_DISPI_MAX_BPP: u16 = 32;

// --- VBE 寄存器索引 (基于 BAR2 + 0x500 的 u16 索引) ---
/// ID 寄存器：读取以校验版本，写入以请求更高版本
pub const VBE_DISPI_INDEX_ID: usize = 0;
/// 宽度寄存器：设置屏幕水平像素数
pub const VBE_DISPI_INDEX_XRES: usize = 1;
/// 高度寄存器：设置屏幕垂直像素数
pub const VBE_DISPI_INDEX_YRES: usize = 2;
/// 色深寄存器：设置 BPP (常用 32)
pub const VBE_DISPI_INDEX_BPP: usize = 3;
/// 使能寄存器：控制显卡开启、LFB 开启及内存保留
pub const VBE_DISPI_INDEX_ENABLE: usize = 4;
/// Bank 寄存器：切换 64K 窗口 (非 LFB 模式使用)
pub const VBE_DISPI_INDEX_BANK: usize = 5;
/// 虚拟宽度：设置逻辑行宽 (用于实现硬件滚屏)
pub const VBE_DISPI_INDEX_VIRT_WIDTH: usize = 6;
/// 虚拟高度：设置逻辑行高
pub const VBE_DISPI_INDEX_VIRT_HEIGHT: usize = 7;
/// X 偏移：设置可见区域在虚拟画布上的起始 X
pub const VBE_DISPI_INDEX_X_OFFSET: usize = 8;
/// Y 偏移：设置可见区域在虚拟画布上的起始 Y
pub const VBE_DISPI_INDEX_Y_OFFSET: usize = 9;
/// 寄存器总数
pub const VBE_DISPI_INDEX_NB: usize = 10;
/// 64K 视频内存块索引 (Legacy)
pub const VBE_DISPI_INDEX_VIDEO_MEMORY_64K: usize = 10;

// --- VBE ID 魔数 (用于 ID 寄存器校验) ---
/// Bochs VBE ID 0 (0xB0C0)
pub const VBE_DISPI_ID0: u16 = 0xB0C0;
pub const VBE_DISPI_ID1: u16 = 0xB0C1;
pub const VBE_DISPI_ID2: u16 = 0xB0C2;
pub const VBE_DISPI_ID3: u16 = 0xB0C3;
pub const VBE_DISPI_ID4: u16 = 0xB0C4;
/// 现代 QEMU 通常支持到 ID 5 (0xB0C5)
pub const VBE_DISPI_ID5: u16 = 0xB0C5;

// --- ENABLE 寄存器位标志 ---
/// 关闭 VBE 扩展 (回归标准 VGA 模式)
pub const VBE_DISPI_DISABLED: u16 = 0;
/// 开启 VBE 扩展
pub const VBE_DISPI_ENABLED: u16 = 1;
/// 获取硬件能力标志 (弃用)
pub const VBE_DISPI_GETCAPS: u16 = 2;
/// 开启 8-bit DAC (用于 256 色模式)
pub const VBE_DISPI_8BIT_DAC: u16 = 32;
/// 【核心】开启线性帧缓冲 (Linear Frame Buffer)
pub const VBE_DISPI_LFB_ENABLED: u16 = 64;
/// 切换模式时不清除显存内容
pub const VBE_DISPI_NOCLEARMEM: u16 = 128;

// --- PCI 空间地址与尺寸定义 ---
/// 默认 LFB 物理地址 (仅供参考，应以 BAR0 为准)
pub const VBE_DISPI_LFB_PHYSICAL_ADDRESS: u32 = 0xE000_0000;
/// PCI BAR2 MMIO 空间总大小 (4KB)
pub const PCI_VGA_MMIO_SIZE: usize = 4096;
/// 内部 I/O 端口偏移 (基于 BAR2)
pub const PCI_VGA_IOPORT_OFFSET: usize = 1024;
/// I/O 端口空间大小
pub const PCI_VGA_IOPORT_SIZE: usize = 32;
/// 【核心】Bochs 寄存器在 BAR2 上的字节偏移 (0x500)
pub const PCI_VGA_BOCHS_OFFSET: usize = 1280;
/// Bochs 寄存器组物理长度
pub const PCI_VGA_BOCHS_SIZE: usize = 22;

// --- QEMU 扩展寄存器 (用于配置字节序等) ---
pub const PCI_VGA_QEXT_OFFSET: usize = 1536;
pub const PCI_VGA_QEXT_SIZE: usize = 8;
pub const PCI_VGA_QEXT_REG_SIZE: u32 = 0;
pub const PCI_VGA_QEXT_REG_BYTEORDER: u32 = 4;
/// QEMU 扩展魔数：小端序
pub const PCI_VGA_QEXT_LITTLE_ENDIAN: u32 = 0x1E1E_1E1E;
/// QEMU 扩展魔数：大端序
pub const PCI_VGA_QEXT_BIG_ENDIAN: u32 = 0xBEBE_BEBE;
