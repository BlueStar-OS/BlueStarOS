use core::arch::asm;

use alloc::vec::Vec;
use log::{debug, error, warn};

use crate::{
    driver::{
        gpu::vga_console::{vga_screen_mut, Console},
        pcie::find_pcie_device,
    },
    fs::fs_backend::new_ext4fs,
    memory::alloc_frame,
};

mod bochs_vbe_const;
pub mod font_matirx;
pub mod vga_console;
pub use font_matirx::get_font_bitmap;
pub use vga_console::*;

/// Doomgeneric 当前默认渲染分辨率宽度。
///
/// 对应上游 `doomgeneric.h` 默认值：
/// `/home/inkbottle/othersrc/doomgeneric/doomgeneric/doomgeneric.h:7-13`
const DOOM_FRAMEBUFFER_WIDTH: u16 = 640;
/// Doomgeneric 当前默认渲染分辨率高度。
const DOOM_FRAMEBUFFER_HEIGHT: u16 = 400;
/// 先固定走 32bpp XRGB8888 路径。
///
/// 参考 Linux 5.4.29:
/// - `drivers/gpu/drm/bochs/bochs_hw.c:207-235`
/// - `drivers/gpu/drm/bochs/bochs_hw.c:237-259`
const DOOM_FRAMEBUFFER_BITS_PER_PIXEL: u16 = 32;
/// VGA Attribute Controller Address/Data Register。
///
/// Linux 在 modeset 前会往 0x3c0 写 0x20 做 unblank。
/// 参考 `drivers/gpu/drm/bochs/bochs_hw.c:220`
const VGA_ATTR_CTL_PORT: usize = 0x3c0;
/// VGA Input Status #1（color emulation）。
///
/// 在标准 VGA 里，先读这个端口会把 Attribute Controller 的 flip-flop 复位为 0，
/// 否则后续对 0x3c0 的写入可能被解释成"属性数据"，而不是"AR index + unblank bit"。
///
/// 参考：
/// - QEMU `hw/display/vga.c:403-408`
/// - QEMU `hw/display/vga.c:430-460`
const VGA_INPUT_STATUS1_COLOR_PORT: usize = 0x3da;

/// 向 BAR2 的 VGA legacy 寄存器窗口写 8 位值。
///
/// Linux 对应实现见 `drivers/gpu/drm/bochs/bochs_hw.c:13-24`。
unsafe fn vga_writeb(mmio_bar2_base_addr: usize, ioport: usize, val: u8) {
    let register_offset = bochs_vbe_const::PCI_VGA_IOPORT_OFFSET + (ioport - VGA_ATTR_CTL_PORT);
    core::ptr::write_volatile((mmio_bar2_base_addr + register_offset) as *mut u8, val);
}

/// 从 BAR2 的 VGA legacy 寄存器窗口读 8 位值。
unsafe fn vga_readb(mmio_bar2_base_addr: usize, ioport: usize) -> u8 {
    let register_offset = bochs_vbe_const::PCI_VGA_IOPORT_OFFSET + (ioport - VGA_ATTR_CTL_PORT);
    core::ptr::read_volatile((mmio_bar2_base_addr + register_offset) as *const u8)
}

/// 读取 Bochs DISPI 16 位寄存器。
unsafe fn vbe_read(mmio_bar2_base_addr: usize, index: usize) -> u16 {
    core::ptr::read_volatile(
        (mmio_bar2_base_addr + bochs_vbe_const::PCI_VGA_BOCHS_OFFSET + (index << 1)) as *const u16,
    )
}

/// 写 Bochs DISPI 16 位寄存器。
unsafe fn vbe_write(mmio_bar2_base_addr: usize, index: usize, val: u16) {
    core::ptr::write_volatile(
        (mmio_bar2_base_addr + bochs_vbe_const::PCI_VGA_BOCHS_OFFSET + (index << 1)) as *mut u16,
        val,
    )
}

/// 访问 QEMU 扩展寄存器。
unsafe fn qext_read(mmio_bar2_base_addr: usize, register_offset: usize) -> u32 {
    core::ptr::read_volatile(
        (mmio_bar2_base_addr + bochs_vbe_const::PCI_VGA_QEXT_OFFSET + register_offset)
            as *const u32,
    )
}

unsafe fn qext_write(mmio_bar2_base_addr: usize, register_offset: usize, val: u32) {
    core::ptr::write_volatile(
        (mmio_bar2_base_addr + bochs_vbe_const::PCI_VGA_QEXT_OFFSET + register_offset) as *mut u32,
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
    // 再写 0x3c0 才能稳定把 bit5 置成"unblank"。
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
    if qext_read(
        mmio_bar2_base_addr,
        bochs_vbe_const::PCI_VGA_QEXT_REG_SIZE as usize,
    ) >= 8
    {
        qext_write(
            mmio_bar2_base_addr,
            bochs_vbe_const::PCI_VGA_QEXT_REG_BYTEORDER as usize,
            bochs_vbe_const::PCI_VGA_QEXT_LITTLE_ENDIAN,
        );
    }

    vbe_write(
        mmio_bar2_base_addr,
        bochs_vbe_const::VBE_DISPI_INDEX_ENABLE,
        0,
    );
    vbe_write(
        mmio_bar2_base_addr,
        bochs_vbe_const::VBE_DISPI_INDEX_BPP,
        bits_per_pixel,
    );
    vbe_write(
        mmio_bar2_base_addr,
        bochs_vbe_const::VBE_DISPI_INDEX_XRES,
        screen_width,
    );
    vbe_write(
        mmio_bar2_base_addr,
        bochs_vbe_const::VBE_DISPI_INDEX_YRES,
        screen_height,
    );
    vbe_write(
        mmio_bar2_base_addr,
        bochs_vbe_const::VBE_DISPI_INDEX_BANK,
        0,
    );
    vbe_write(
        mmio_bar2_base_addr,
        bochs_vbe_const::VBE_DISPI_INDEX_VIRT_WIDTH,
        screen_width,
    );
    vbe_write(
        mmio_bar2_base_addr,
        bochs_vbe_const::VBE_DISPI_INDEX_VIRT_HEIGHT,
        virtual_height,
    );
    vbe_write(
        mmio_bar2_base_addr,
        bochs_vbe_const::VBE_DISPI_INDEX_X_OFFSET,
        0,
    );
    vbe_write(
        mmio_bar2_base_addr,
        bochs_vbe_const::VBE_DISPI_INDEX_Y_OFFSET,
        0,
    );
    vbe_write(
        mmio_bar2_base_addr,
        bochs_vbe_const::VBE_DISPI_INDEX_ENABLE,
        bochs_vbe_const::VBE_DISPI_ENABLED | bochs_vbe_const::VBE_DISPI_LFB_ENABLED,
    );

    asm!("fence iorw, iorw");
}

/// 向 framebuffer 写单个 32bpp 像素。
pub unsafe fn put_pixel(
    framebuffer_base_addr: usize,
    screen_width: usize,
    pixel_x: usize,
    pixel_y: usize,
    pixel_color: u32,
) {
    let pixel_ptr = (framebuffer_base_addr as *mut u32).add(pixel_y * screen_width + pixel_x);
    core::ptr::write_volatile(pixel_ptr, pixel_color);
}

/// 初始化 QEMU VGA。
///
/// 当前先做最小验证：
/// 1. 读出 BAR0 framebuffer 和 BAR2 MMIO；
/// 2. 校验 Bochs ID；
/// 3. 做一次 640x400x32 modeset；
/// 4. 初始化全局 `VGA_SCREEN`，供 `/dev/fb0` 和 VGA 控制台共享。
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
            framebuffer_base_addr, mmio_bar2_base_addr, framebuffer_size
        );

        unsafe {
            let bochs_id = vbe_read(mmio_bar2_base_addr, bochs_vbe_const::VBE_DISPI_INDEX_ID);
            let video_memory_64k = vbe_read(
                mmio_bar2_base_addr,
                bochs_vbe_const::VBE_DISPI_INDEX_VIDEO_MEMORY_64K,
            );
            debug!(
                "bochs id={:#x}, video_memory={:#x} KiB",
                bochs_id,
                video_memory_64k as usize * 64
            );

            if (bochs_id & 0xfff0) != bochs_vbe_const::VBE_DISPI_ID0
                || bochs_id > bochs_vbe_const::VBE_DISPI_ID5
            {
                warn!("unexpected bochs id {:#x}", bochs_id);
            }

            bochs_modeset(
                mmio_bar2_base_addr,
                framebuffer_size,
                DOOM_FRAMEBUFFER_WIDTH,
                DOOM_FRAMEBUFFER_HEIGHT,
                DOOM_FRAMEBUFFER_BITS_PER_PIXEL,
            );

            debug!(
                "after modeset: enable={:#x} xres={} yres={} bpp={} virt_width={} virt_height={}",
                vbe_read(mmio_bar2_base_addr, bochs_vbe_const::VBE_DISPI_INDEX_ENABLE),
                vbe_read(mmio_bar2_base_addr, bochs_vbe_const::VBE_DISPI_INDEX_XRES),
                vbe_read(mmio_bar2_base_addr, bochs_vbe_const::VBE_DISPI_INDEX_YRES),
                vbe_read(mmio_bar2_base_addr, bochs_vbe_const::VBE_DISPI_INDEX_BPP),
                vbe_read(
                    mmio_bar2_base_addr,
                    bochs_vbe_const::VBE_DISPI_INDEX_VIRT_WIDTH
                ),
                vbe_read(
                    mmio_bar2_base_addr,
                    bochs_vbe_const::VBE_DISPI_INDEX_VIRT_HEIGHT
                ),
            );

            asm!("fence iorw, iorw");
            *vga_screen_mut() = Console {
                fb_base: framebuffer_base_addr as *mut u32,
                width: DOOM_FRAMEBUFFER_WIDTH as usize,
                height: DOOM_FRAMEBUFFER_HEIGHT as usize,
                font_x: 8,
                font_y: 16,
                cursor_x: 0,
                cursor_y: 0,
                foreground: 0x00FF_FFFF,
                background: 0x0000_0000,
                bold: false,
                state: AnsiState::Normal,
                soft_buffer: Vec::new(),
            };
            let soft_buffer_frame = alloc_frame().expect("VGA buffer alloc fail!");
            vga_screen_mut().soft_buffer.push(soft_buffer_frame)
        }
    }
}
