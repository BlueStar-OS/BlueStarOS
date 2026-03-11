use spin::Mutex;
use virtio_drivers::{Hal, VirtIOBlk, VirtIOHeader};
use lazy_static::*;
use alloc::{sync::Arc, vec::Vec};
use crate::{memory::*};
use crate::sync::UPSafeCell;
use crate::kprintln;
use virtio_drivers::VirtIOGpu;
use crate::arch::memory::*;
const MMIO_BASE: usize = 0x10001000; //QEMU virtio mmio base address
const MMIO_STRIDE: usize = 0x1000; //QEMU virtio mmio stride
const MMIO_END: usize = 0x10008000; //QEMU virtio mmio end address

lazy_static!{
    static ref QUEUE_FRAMES:UPSafeCell<Vec<(virtio_drivers::PhysAddr, Vec<FramTracker>)>> =
        UPSafeCell::new(Vec::new());
}


pub fn init_gpu() {
    kprintln!("Scanning for VirtIO GPU...");

    // 1. 遍历 MMIO 区域寻找 GPU 设备
    for addr in (MMIO_BASE..MMIO_END).step_by(MMIO_STRIDE) {
        let header = unsafe { &mut *(addr as *mut VirtIOHeader) };
        
        // 验证魔数 "virt" (0x74726976) 且设备 ID 为 16 (GPU)
        if header.verify() && header.device_type() == virtio_drivers::DeviceType::GPU {
            kprintln!("Found VirtIO GPU at address: {:#x}", addr);
            
            // 2. 初始化 GPU 驱动
            let mut gpu = VirtIOGpu::<VirtioHal>::new(header)
                .expect("Failed to create GPU driver");

            let raw_ref= &mut gpu as *mut VirtIOGpu<VirtioHal>;
            
            // 获取 framebuffer (这是显存的一块映射切片)
            let mut fb = gpu.setup_framebuffer()
                .expect("Failed to setup framebuffer");

            // 3. 获取显示信息并设置 Framebuffer
            // 设置分辨率，例如 800x600
              
            let (width, height) = unsafe { (raw_ref.as_mut()).unwrap().resolution() };
            kprintln!("Framebuffer resolution: {}x{}", width, height);
            
            // Framebuffer 通常是 RGBA 或 BGRA 格式，每个像素 4 字节
            // slice 长度 = width * height * 4
            
            kprintln!("Drawing red square...");
            draw_red_square(&mut fb, width, height);

            // 4. 重要：刷新缓冲区到屏幕
            // 告诉 GPU 将显存内容推送到 QEMU 窗口
            gpu.flush().expect("Failed to flush GPU");
            
            kprintln!("Done!");
            return;
        }
    }
    kprintln!("VirtIO GPU not found!");
}

fn draw_red_square(fb: &mut [u8], screen_width: u32, screen_height: u32) {
    let width = screen_width;
    let height = screen_height;
  // --------------------------------------------------------
    // 1. 先把背景全部涂成白色，确保我们知道屏幕边界在哪里
    // --------------------------------------------------------
    for i in (0..fb.len()).step_by(4) {
        fb[i] = 255;     // B
        fb[i+1] = 255;   // G
        fb[i+2] = 255;   // R
        fb[i+3] = 0;     // A (有时候 Alpha=0 会导致透视黑屏，保险起见可以设 255，但在 QEMU 0 通常没事)
    }

    // --------------------------------------------------------
    // 2. 画红色正方形 (修正为 BGRA 格式)
    // --------------------------------------------------------
    let sq_x = 200;
    let sq_y = 150;
    let sq_size = 200;

    for y in sq_y..(sq_y + sq_size) {
        for x in sq_x..(sq_x + sq_size) {
            if x >= width || y >= height { continue; }

            // 这里的 width 必须和 setup_framebuffer 传入的 width 严格一致
            let idx = ((y * width + x) * 4) as usize;

            // 边界安全检查（防止 panic）
            if idx + 3 < fb.len() {
                // 修正：VirtIO GPU 通常是 BGRA 格式
                fb[idx] = 0;       // Blue
                fb[idx + 1] = 0;   // Green
                fb[idx + 2] = 255; // Red
                fb[idx + 3] = 255; // Alpha
            }
        }
    }
}


pub struct VirtBlk(pub UPSafeCell<VirtIOBlk<'static,VirtioHal>>, u64);



impl VirtBlk {
    /// TODO:扫描mmio遍历设备
    /// 添加到全局块设备里面
    pub fn new()->Self{
        unsafe {
            let header = &mut *(MMIO_BASE as *mut VirtIOHeader);
            let capacity_in_sectors = core::ptr::read_volatile(header.config_space() as *const u64);
            let blk = VirtBlk(
                UPSafeCell::new(
                    VirtIOBlk::new(header).expect("failed new blk device")
                ),
                capacity_in_sectors,
            );
            blk
        }
    }

    pub fn capacity_in_sectors(&self) -> u64 {
        self.1
    }
}

pub struct VirtioHal;
impl Hal for VirtioHal {
    fn dma_alloc(pages: usize) -> virtio_drivers::PhysAddr {
        let frames = alloc_contiguous_frames(pages).expect("no contiguous frames alloced");
        let base_ppn = frames
            .first()
            .map(|f| f.ppn)
            .unwrap_or(PhysiNumber(0));
        let base_addr: PhysiAddr = base_ppn.into();

        unsafe {
            let len = pages * crate::config::PAGE_SIZE;
            core::slice::from_raw_parts_mut(base_addr.0 as *mut u8, len).fill(0);
        }

        QUEUE_FRAMES.lock().push((base_addr.0, frames));
        base_addr.0
    }
    fn dma_dealloc(paddr: virtio_drivers::PhysAddr, pages: usize) -> i32 {
        let mut q = QUEUE_FRAMES.lock();
        if let Some(pos) = q.iter().position(|(base, v)| *base == paddr && v.len() == pages) {
            let (_, frames) = q.remove(pos);
            drop(frames);
            0
        } else {
            -1
        }
    }
    fn phys_to_virt(paddr: virtio_drivers::PhysAddr) -> virtio_drivers::VirtAddr {
        paddr
    }
    fn virt_to_phys(vaddr: virtio_drivers::VirtAddr) -> virtio_drivers::PhysAddr {
        let mut table = PageTable::get_kernel_table_layer();
        if let Some(paddr) = table.translate(VirAddr(vaddr)) {
            paddr.0
        } else {
            // Fallback to identity mapping for early-boot / direct-mapped regions.
            vaddr
        }
    }
}

