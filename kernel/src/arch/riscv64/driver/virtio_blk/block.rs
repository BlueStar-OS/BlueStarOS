use lazy_static::*;
use log::{debug, info};
use virtio_drivers::{DeviceType, Hal, VirtIOBlk, VirtIOGpu, VirtIOHeader};

use alloc::vec::Vec;

use crate::arch::memory::*;
use crate::driver::dtb::DeviceNode;
use crate::kprintln;
use crate::memory::*;
use crate::sync::UPSafeCell;

const VIRTIO_DEVICE_BLOCK_ID: u32 = 2;
const VIRTIO_DEVICE_GPU_ID: u32 = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct VirtioMmioDevice {
    base: usize,
    device_id: u32,
}

lazy_static! {
    static ref QUEUE_FRAMES: UPSafeCell<Vec<(virtio_drivers::PhysAddr, Vec<FramTracker>)>> =
        UPSafeCell::new(Vec::new());
    static ref VIRTIO_MMIO_DEVICES: UPSafeCell<Vec<VirtioMmioDevice>> =
        UPSafeCell::new(Vec::new());
}

fn virtio_device_name(device_id: u32) -> &'static str {
    match device_id {
        VIRTIO_DEVICE_BLOCK_ID => "block",
        VIRTIO_DEVICE_GPU_ID => "gpu",
        1 => "network",
        3 => "console",
        _ => "unknown",
    }
}

fn record_virtio_mmio_device(device: VirtioMmioDevice) {
    let mut devices = VIRTIO_MMIO_DEVICES.lock();
    if devices.iter().any(|existing| existing.base == device.base) {
        return;
    }
    devices.push(device);
}

fn find_virtio_mmio_device(device_id: u32) -> Option<VirtioMmioDevice> {
    let devices = VIRTIO_MMIO_DEVICES.lock();
    devices.iter().copied().find(|device| device.device_id == device_id)
}

fn virtio_mmio_probe(node: &DeviceNode, _compatible: &str) -> Result<(), &'static str> {
    let reg = node.get_property("reg").ok_or("Missing reg property")?;
    let regs = reg.as_reg(2, 2);
    if regs.is_empty() {
        return Err("Empty reg property");
    }

    let base_addr = regs[0].address as usize;
    let size = regs[0].size as usize;
    if size == 0 {
        return Err("VirtIO MMIO size is zero");
    }

    let mmio_range = VirNumRange::new(VirAddr(base_addr), VirAddr(base_addr + size - 1));
    let flags = MapAreaFlags::V | MapAreaFlags::R | MapAreaFlags::W
        | MapAreaFlags::A | MapAreaFlags::G | MapAreaFlags::DEV;
    register_kernel_mmio(mmio_range, flags);

    let header = unsafe { &*(base_addr as *const VirtIOHeader) };
    if !header.verify() {
        return Err("VirtIO MMIO header verify failed");
    }

    let device_type = header.device_type();
    let device_id = device_type as u8 as u32;
    info!(
        "[VirtIO Probe] Found {} device at {:#x}, size={:#x}",
        virtio_device_name(device_id),
        base_addr,
        size
    );

    record_virtio_mmio_device(VirtioMmioDevice {
        base: base_addr,
        device_id,
    });
    Ok(())
}

crate::dtb_probe! {
    compatible: "virtio,mmio",
    priority: Mid,
    driver: "virtio-mmio",
    probe: virtio_mmio_probe
}

pub fn init_gpu() {
    kprintln!("Scanning for VirtIO GPU...");

    let Some(device) = find_virtio_mmio_device(VIRTIO_DEVICE_GPU_ID) else {
        kprintln!("VirtIO GPU not found via DTB");
        return;
    };

    let header = unsafe { &mut *(device.base as *mut VirtIOHeader) };
    if header.device_type() != DeviceType::GPU {
        kprintln!("VirtIO GPU probe mismatch at {:#x}", device.base);
        return;
    }

    kprintln!("Found VirtIO GPU at address: {:#x}", device.base);

    let mut gpu = VirtIOGpu::<VirtioHal>::new(header)
        .expect("Failed to create GPU driver");
    let raw_ref = &mut gpu as *mut VirtIOGpu<VirtioHal>;
    let mut fb = gpu
        .setup_framebuffer()
        .expect("Failed to setup framebuffer");

    let (width, height) = unsafe { raw_ref.as_mut().unwrap().resolution() };
    kprintln!("Framebuffer resolution: {}x{}", width, height);
    kprintln!("Drawing red square...");
    draw_red_square(&mut fb, width, height);
    gpu.flush().expect("Failed to flush GPU");
    kprintln!("Done!");
}

fn draw_red_square(fb: &mut [u8], screen_width: u32, screen_height: u32) {
    let width = screen_width;
    let height = screen_height;
    for i in (0..fb.len()).step_by(4) {
        fb[i] = 255;
        fb[i + 1] = 255;
        fb[i + 2] = 255;
        fb[i + 3] = 0;
    }

    let sq_x = 200;
    let sq_y = 150;
    let sq_size = 200;

    for y in sq_y..(sq_y + sq_size) {
        for x in sq_x..(sq_x + sq_size) {
            if x >= width || y >= height {
                continue;
            }

            let idx = ((y * width + x) * 4) as usize;
            if idx + 3 < fb.len() {
                fb[idx] = 0;
                fb[idx + 1] = 0;
                fb[idx + 2] = 255;
                fb[idx + 3] = 255;
            }
        }
    }
}

pub struct VirtBlk(pub UPSafeCell<VirtIOBlk<'static, VirtioHal>>, u64);

unsafe impl Send for VirtBlk {}
unsafe impl Sync for VirtBlk {}

impl crate::fs::vfs::BlueBlk for VirtBlk {
    fn read_block(
        &mut self,
        lba: usize,
        buf: &mut [u8],
    ) -> Result<(), crate::fs::vfs::VfsFsError> {
        self.0
            .lock()
            .read_block(lba, buf)
            .map_err(|_| crate::fs::vfs::VfsFsError::IO)
    }

    fn write_block(&mut self, lba: usize, buf: &[u8]) -> Result<(), crate::fs::vfs::VfsFsError> {
        self.0
            .lock()
            .write_block(lba, buf)
            .map_err(|_| crate::fs::vfs::VfsFsError::IO)
    }

    fn capacity_in_sectors(&self) -> u64 {
        self.1
    }
}

impl VirtBlk {
    pub fn new() -> Self {
        Self::new_from_dtb().expect("VirtIO block MMIO device not discovered via DTB probe")
    }

    pub fn new_from_dtb() -> Result<Self, &'static str> {
        let device = find_virtio_mmio_device(VIRTIO_DEVICE_BLOCK_ID)
            .ok_or("No virtio block MMIO device found")?;

        unsafe {
            debug!("New VirtBlk from DTB at {:#x}", device.base);
            let header = &mut *(device.base as *mut VirtIOHeader);
            if header.device_type() != DeviceType::Block {
                return Err("Probed VirtIO MMIO device is not a block device");
            }

            let capacity_in_sectors =
                core::ptr::read_volatile(header.config_space() as *const u64);
            Ok(VirtBlk(
                UPSafeCell::new(
                    VirtIOBlk::new(header).map_err(|_| "failed new blk device")?,
                ),
                capacity_in_sectors,
            ))
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
            vaddr
        }
    }
}
