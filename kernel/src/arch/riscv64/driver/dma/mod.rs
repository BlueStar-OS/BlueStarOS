//! Minimal DMA memory support for RISC-V.
//!
//! The kernel currently uses identity-mapped physical memory, so a DMA buffer
//! can use the same value as both its CPU pointer and its device address.  The
//! allocator deliberately returns whole, contiguous 4 KiB pages because that
//! is also the page size used by the xHCI controller in this tree.
//!
//! RISC-V `fence` only orders accesses; it does not make a non-coherent cache
//! visible to a device.  The Zicbom operations below do the cache maintenance:
//! `cbo.clean` before a device reads memory and `cbo.inval` before the CPU reads
//! memory written by a device.

use crate::arch::memory::{PhysiAddr, VirAddr};
use crate::config::PAGE_SIZE;
use crate::memory::{alloc_contiguous_frames, FramTracker};
use alloc::vec::Vec;
use core::arch::asm;
/// The cache-block size exposed by the RISC-V QEMU and K3 DTBs in this tree.
///
/// Zicbom operates on cache blocks.  Both supported RISC-V machine profiles
/// report 64 bytes (`riscv,cbom-block-size`/`riscv,cbop-block-size`).
pub const CACHE_BLOCK_SIZE: usize = 64;

/// Allocate contiguous, identity-mapped DMA memory.
pub struct DmaMemory {
    /// Owns the backing frames until the DMA object is dropped.
    frames: Vec<FramTracker>,
    /// Allocation size after rounding up to whole pages.
    len: usize,
    /// CPU virtual address of the first byte in the allocation.
    pub cpu_addr: VirAddr,
    /// Physical address presented to the DMA device.
    pub dma_addr: PhysiAddr,
}

impl DmaMemory {
    /// Allocate at least `len` bytes, rounded up to whole 4 KiB pages.
    pub fn new(len: usize) -> Option<Self> {
        if len == 0 {
            return None;
        }

        let pages = len.div_ceil(PAGE_SIZE);
        let frames = alloc_contiguous_frames(pages)?;
        let dma_addr = PhysiAddr(frames[0].ppn.0 * PAGE_SIZE);
        Some(Self {
            frames,
            len: pages * PAGE_SIZE,
            cpu_addr: VirAddr(dma_addr.0),
            dma_addr,
        })
    }

    /// The physical address consumed by a DMA-capable device.
    pub fn phys_addr(&self) -> usize {
        self.dma_addr.0
    }

    /// The identity-mapped CPU pointer for this allocation.
    pub fn as_mut_ptr(&self) -> *mut u8 {
        self.cpu_addr.0 as *mut u8
    }

    /// Size of the allocation, including page rounding.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Return a typed pointer into the identity-mapped allocation.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `offset..offset + size_of::<T>()` is inside
    /// this allocation and that the pointer is correctly aligned for `T`.
    pub unsafe fn as_ptr<T>(&self, offset: usize) -> *mut T {
        self.as_mut_ptr().add(offset).cast()
    }

    /// Clean CPU cache lines so a device can read the latest CPU writes.
    pub fn clean_for_device(&self) {
        // Order descriptor stores before the cache clean itself.
        memory_barrier();
        clean_cache_range(self.as_mut_ptr() as usize, self.len);
        dma_write_barrier();
    }

    /// Invalidate CPU cache lines so the CPU observes device writes.
    pub fn invalidate_for_cpu(&self) {
        dma_read_barrier();
        invalidate_cache_range(self.as_mut_ptr() as usize, self.len);
        // CBO.INVAL is ordered before a following load by Zicbom's PPO rules.
        // Keep an explicit fence here as a cheap compiler/CPU ordering point
        // for callers that immediately inspect a descriptor or event TRB.
        memory_barrier();
    }
}

/// Order normal-memory writes before a following MMIO/device operation.
#[inline(always)]
pub fn dma_write_barrier() {
    // The `io` successor set is intentional: a ring must be visible before a
    // doorbell/register write tells the controller to fetch it.
    unsafe { asm!("fence rw, io", options(nostack, preserves_flags)) }
}

/// Order a device/MMIO completion observation before normal-memory reads.
#[inline(always)]
pub fn dma_read_barrier() {
    unsafe { asm!("fence io, rw", options(nostack, preserves_flags)) }
}

#[inline(always)]
fn memory_barrier() {
    unsafe { asm!("fence rw, rw", options(nostack, preserves_flags)) }
}

fn cache_range(start: usize, len: usize) -> (usize, usize) {
    assert!(len > 0);
    let first = start & !(CACHE_BLOCK_SIZE - 1);
    let end = start.checked_add(len).expect("DMA range overflow");
    let last_exclusive = end.div_ceil(CACHE_BLOCK_SIZE) * CACHE_BLOCK_SIZE;
    (first, last_exclusive)
}

fn clean_cache_range(start: usize, len: usize) {
    let (first, end) = cache_range(start, len);
    let mut address = first;
    while address < end {
        cache_clean(address);
        address += CACHE_BLOCK_SIZE;
    }
}

fn invalidate_cache_range(start: usize, len: usize) {
    let (first, end) = cache_range(start, len);
    let mut address = first;
    while address < end {
        cache_invalidate(address);
        address += CACHE_BLOCK_SIZE;
    }
}

/// Execute Zicbom `cbo.clean` for one cache block.
#[inline(always)]
fn cache_clean(address: usize) {
    unsafe {
        asm!(
            "cbo.clean 0({address})",
            address = in(reg) address,
            options(nostack, preserves_flags)
        )
    }
}

/// Execute Zicbom `cbo.inval` for one cache block.
#[inline(always)]
fn cache_invalidate(address: usize) {
    unsafe {
        asm!(
            "cbo.inval 0({address})",
            address = in(reg) address,
            options(nostack, preserves_flags)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{cache_range, CACHE_BLOCK_SIZE};

    #[test]
    fn dma_range_covers_partial_cache_blocks() {
        assert_eq!(
            cache_range(CACHE_BLOCK_SIZE + 3, CACHE_BLOCK_SIZE + 1),
            (CACHE_BLOCK_SIZE, CACHE_BLOCK_SIZE * 3)
        );
    }
}
