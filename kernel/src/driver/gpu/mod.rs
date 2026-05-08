use core::arch::asm;

use log::{debug, error, warn};

use crate::{driver::pcie::find_pcie_device, fs::fs_backend::new_ext4fs};

/// BAR2 中 VGA legacy 寄存器窗口起始偏移。
///
/// 参考：
/// - Linux 5.4.29 `drivers/gpu/drm/bochs/bochs_hw.c:18-24`
/// - QEMU `include/hw/display/bochs-vbe.h:51-57`
const VGA_MMIO_OFF: usize = 0x400;
/// BAR2 中 Bochs DISPI 寄存器窗口起始偏移。
///
/// 参考：
/// - Linux 5.4.29 `drivers/gpu/drm/bochs/bochs_hw.c:30-44`
/// - QEMU `include/hw/display/bochs-vbe.h:55-57`
const BOCHS_MMIO_OFF: usize = 0x500;
/// BAR2 中 QEMU 扩展寄存器窗口起始偏移。
///
/// 参考 QEMU `include/hw/display/bochs-vbe.h:59-67`
const QEXT_MMIO_OFF: usize = 0x600;

/// VGA Attribute Controller Address/Data Register。
///
/// Linux 在 modeset 前会往 0x3c0 写 0x20 做 unblank。
/// 参考 `drivers/gpu/drm/bochs/bochs_hw.c:220`
const VGA_ATTR_CTL_PORT: usize = 0x3c0;
/// VGA Input Status #1（color emulation）。
///
/// 在标准 VGA 里，先读这个端口会把 Attribute Controller 的 flip-flop 复位为 0，
/// 否则后续对 0x3c0 的写入可能被解释成“属性数据”，而不是“AR index + unblank bit”。
///
/// 参考：
/// - QEMU `hw/display/vga.c:403-408`
/// - QEMU `hw/display/vga.c:430-460`
const VGA_INPUT_STATUS1_COLOR_PORT: usize = 0x3da;

/// Bochs DISPI 寄存器索引。
const VBE_DISPI_INDEX_ID: usize = 0;
const VBE_DISPI_INDEX_XRES: usize = 1;
const VBE_DISPI_INDEX_YRES: usize = 2;
const VBE_DISPI_INDEX_BPP: usize = 3;
const VBE_DISPI_INDEX_ENABLE: usize = 4;
const VBE_DISPI_INDEX_BANK: usize = 5;
const VBE_DISPI_INDEX_VIRT_WIDTH: usize = 6;
const VBE_DISPI_INDEX_VIRT_HEIGHT: usize = 7;
const VBE_DISPI_INDEX_X_OFFSET: usize = 8;
const VBE_DISPI_INDEX_Y_OFFSET: usize = 9;
const VBE_DISPI_INDEX_VIDEO_MEMORY_64K: usize = 10;

/// Bochs/QEMU VBE 设备 ID。
///
/// 参考：
/// - Linux 5.4.29 `drivers/gpu/drm/bochs/bochs.h:32-37`
/// - QEMU `include/hw/display/bochs-vbe.h:25-31`
const VBE_DISPI_ID0: u16 = 0xB0C0;
const VBE_DISPI_ID5: u16 = 0xB0C5;

/// ENABLE 寄存器控制位。
const VBE_DISPI_ENABLED: u16 = 0x01;
const VBE_DISPI_LFB_ENABLED: u16 = 0x40;

/// QEMU 扩展寄存器：framebuffer byteorder。
///
/// 参考：
/// - Linux 5.4.29 `drivers/gpu/drm/bochs/bochs_hw.c:59-64`
/// - QEMU `include/hw/display/bochs-vbe.h:63-67`
const PCI_VGA_QEXT_REG_SIZE: usize = 0x0;
const PCI_VGA_QEXT_REG_BYTEORDER: usize = 0x4;
const PCI_VGA_QEXT_LITTLE_ENDIAN: u32 = 0x1e1e1e1e;

/// 向 BAR2 的 VGA legacy 寄存器窗口写 8 位值。
///
/// Linux 对应实现见 `drivers/gpu/drm/bochs/bochs_hw.c:13-24`。
unsafe fn vga_writeb(mmio_bar2_base_addr: usize, ioport: usize, val: u8) {
    let register_offset = VGA_MMIO_OFF + (ioport - VGA_ATTR_CTL_PORT);
    core::ptr::write_volatile((mmio_bar2_base_addr + register_offset) as *mut u8, val);
}

/// 从 BAR2 的 VGA legacy 寄存器窗口读 8 位值。
unsafe fn vga_readb(mmio_bar2_base_addr: usize, ioport: usize) -> u8 {
    let register_offset = VGA_MMIO_OFF + (ioport - VGA_ATTR_CTL_PORT);
    core::ptr::read_volatile((mmio_bar2_base_addr + register_offset) as *const u8)
}

/// 读取 Bochs DISPI 16 位寄存器。
unsafe fn vbe_read(mmio_bar2_base_addr: usize, index: usize) -> u16 {
    core::ptr::read_volatile(
        (mmio_bar2_base_addr + BOCHS_MMIO_OFF + (index << 1)) as *const u16,
    )
}

/// 写 Bochs DISPI 16 位寄存器。
unsafe fn vbe_write(mmio_bar2_base_addr: usize, index: usize, val: u16) {
    core::ptr::write_volatile(
        (mmio_bar2_base_addr + BOCHS_MMIO_OFF + (index << 1)) as *mut u16,
        val,
    )
}

/// 访问 QEMU 扩展寄存器。
unsafe fn qext_read(mmio_bar2_base_addr: usize, register_offset: usize) -> u32 {
    core::ptr::read_volatile((mmio_bar2_base_addr + QEXT_MMIO_OFF + register_offset) as *const u32)
}

unsafe fn qext_write(mmio_bar2_base_addr: usize, register_offset: usize, val: u32) {
    core::ptr::write_volatile(
        (mmio_bar2_base_addr + QEXT_MMIO_OFF + register_offset) as *mut u32,
        val,
    )
}

/// 按 Linux bochs 驱动的顺序做一次最小 modeset。
///
/// 流程：
/// 1. 先对 VGA Attribute Controller 做 unblank；
/// 2. 如果有 qext，显式切到 little-endian framebuffer；
/// 3. 再写 DISPI 模式寄存器。
///
/// 参考：
/// - Linux 5.4.29 `drivers/gpu/drm/bochs/bochs_hw.c:51-70`
/// - Linux 5.4.29 `drivers/gpu/drm/bochs/bochs_hw.c:207-235`
unsafe fn bochs_modeset(
    mmio_bar2_base_addr: usize,
    framebuffer_size: usize,
    screen_width: u16,
    screen_height: u16,
    bits_per_pixel: u16,
) {
    let bytes_per_pixel = (bits_per_pixel as usize) / 8;
    let stride_bytes = screen_width as usize * bytes_per_pixel;
    let virtual_height = (framebuffer_size / stride_bytes) as u16;

    // 对标准 VGA 路径，先读 0x3da 复位 Attribute Controller flip-flop，
    // 再写 0x3c0 才能稳定把 bit5 置成“unblank”。
    //
    // QEMU `vga_update_display()` 会先看 `ar_index & 0x20`；
    // 如果没置位，就直接进入 `GMODE_BLANK`。
    // 参考：
    // - `hw/display/vga.c:403-408`
    // - `hw/display/vga.c:430-460`
    // - `hw/display/vga.c:1797-1816`
    let _ = vga_readb(mmio_bar2_base_addr, VGA_INPUT_STATUS1_COLOR_PORT);
    vga_writeb(mmio_bar2_base_addr, VGA_ATTR_CTL_PORT, 0x20);

    // 小端 RISC-V 应与 Linux 一样把 qext byteorder 设为 little-endian。
    if qext_read(mmio_bar2_base_addr, PCI_VGA_QEXT_REG_SIZE) >= 8 {
        qext_write(
            mmio_bar2_base_addr,
            PCI_VGA_QEXT_REG_BYTEORDER,
            PCI_VGA_QEXT_LITTLE_ENDIAN,
        );
    }

    vbe_write(mmio_bar2_base_addr, VBE_DISPI_INDEX_ENABLE, 0);
    vbe_write(mmio_bar2_base_addr, VBE_DISPI_INDEX_BPP, bits_per_pixel);
    vbe_write(mmio_bar2_base_addr, VBE_DISPI_INDEX_XRES, screen_width);
    vbe_write(mmio_bar2_base_addr, VBE_DISPI_INDEX_YRES, screen_height);
    vbe_write(mmio_bar2_base_addr, VBE_DISPI_INDEX_BANK, 0);
    vbe_write(mmio_bar2_base_addr, VBE_DISPI_INDEX_VIRT_WIDTH, screen_width);
    vbe_write(mmio_bar2_base_addr, VBE_DISPI_INDEX_VIRT_HEIGHT, virtual_height);
    vbe_write(mmio_bar2_base_addr, VBE_DISPI_INDEX_X_OFFSET, 0);
    vbe_write(mmio_bar2_base_addr, VBE_DISPI_INDEX_Y_OFFSET, 0);
    vbe_write(
        mmio_bar2_base_addr,
        VBE_DISPI_INDEX_ENABLE,
        VBE_DISPI_ENABLED | VBE_DISPI_LFB_ENABLED,
    );

    asm!("fence iorw, iorw");
}

/// 向 framebuffer 写单个 32bpp 像素。
unsafe fn put_pixel(
    framebuffer_base_addr: usize,
    screen_width: usize,
    pixel_x: usize,
    pixel_y: usize,
    pixel_color: u32,
) {
    let pixel_ptr = (framebuffer_base_addr as *mut u32).add(pixel_y * screen_width + pixel_x);
    core::ptr::write_volatile(pixel_ptr, pixel_color);
}

/// 填充一个明显可见的大矩形，避免只画少量像素时误判“没有显示”。
unsafe fn fill_rect(
    framebuffer_base_addr: usize,
    screen_width: usize,
    start_x: usize,
    start_y: usize,
    rect_width: usize,
    rect_height: usize,
    pixel_color: u32,
) {
    for pixel_y in start_y..(start_y + rect_height) {
        for pixel_x in start_x..(start_x + rect_width) {
            put_pixel(
                framebuffer_base_addr,
                screen_width,
                pixel_x,
                pixel_y,
                pixel_color,
            );
        }
    }
}

/// 初始化 QEMU VGA。
///
/// 当前先做最小验证：
/// 1. 读出 BAR0 framebuffer 和 BAR2 MMIO；
/// 2. 校验 Bochs ID；
/// 3. 做一次 256x256x32 modeset；
/// 4. 在左上角画一个 128x128 纯绿色方块。
pub fn inital_gpu() {
    if let Some(vga_device) = find_pcie_device(0x1234, 0x1111) {
        warn!("Already get vga pcie device");

        let Some(framebuffer_bar) = vga_device
            .bars
            .iter()
            .find(|bar_info| bar_info.bar_index == 0)
        else {
            error!("bad vga device, missing framebuffer BAR0");
            return;
        };

        let Some(mmio_control_bar) = vga_device
            .bars
            .iter()
            .find(|bar_info| bar_info.bar_index == 2)
        else {
            error!("bad vga device, missing control BAR2");
            return;
        };

        let framebuffer_base_addr = framebuffer_bar.base_addr as usize;
        let framebuffer_size = framebuffer_bar.size as usize;
        let mmio_bar2_base_addr = mmio_control_bar.base_addr as usize;

        debug!(
            "vga framebuffer={:#x}, bar2 mmio={:#x}, fb_size={:#x}",
            framebuffer_base_addr,
            mmio_bar2_base_addr,
            framebuffer_size
        );

        unsafe {
            let bochs_id = vbe_read(mmio_bar2_base_addr, VBE_DISPI_INDEX_ID);
            let video_memory_64k =
                vbe_read(mmio_bar2_base_addr, VBE_DISPI_INDEX_VIDEO_MEMORY_64K);
            debug!(
                "bochs id={:#x}, video_memory={:#x} KiB",
                bochs_id,
                video_memory_64k as usize * 64
            );

            if (bochs_id & 0xfff0) != VBE_DISPI_ID0 || bochs_id > VBE_DISPI_ID5 {
                warn!("unexpected bochs id {:#x}", bochs_id);
            }

            bochs_modeset(mmio_bar2_base_addr, framebuffer_size, 256, 256, 32);

            debug!(
                "after modeset: enable={:#x} xres={} yres={} bpp={} virt_width={} virt_height={}",
                vbe_read(mmio_bar2_base_addr, VBE_DISPI_INDEX_ENABLE),
                vbe_read(mmio_bar2_base_addr, VBE_DISPI_INDEX_XRES),
                vbe_read(mmio_bar2_base_addr, VBE_DISPI_INDEX_YRES),
                vbe_read(mmio_bar2_base_addr, VBE_DISPI_INDEX_BPP),
                vbe_read(mmio_bar2_base_addr, VBE_DISPI_INDEX_VIRT_WIDTH),
                vbe_read(mmio_bar2_base_addr, VBE_DISPI_INDEX_VIRT_HEIGHT),
            );

            // 画一个足够大的绿色方块，避免只有一条 scanline 不易观察。
            fill_rect(framebuffer_base_addr, 256, 0, 0, 128, 128, 0x0000_ff00);
            asm!("fence iorw, iorw");
        }
    }
}
