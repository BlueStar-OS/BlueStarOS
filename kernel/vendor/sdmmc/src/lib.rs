//! # SD/MMC/eMMC Driver Library
//!
//! This library provides a no_std driver for SD, MMC, and eMMC storage devices
//! on ARM64 platforms, specifically optimized for Rockchip RK3568/RK3588 platforms.
//!
//! ## Features
//!
//! - **no_std compatible**: Works without the standard library
//! - **eMMC 4.x/5.x support**: Full support for eMMC standards including HS200/HS400 modes
//! - **SD/SDIO support**: Support for SD 1.0/2.0 and SDIO devices
//! - **PIO and DMA modes**: Choose between programmed I/O or DMA for data transfers
//! - **Rockchip optimization**: Specifically optimized for Rockchip platforms with DWCMSHC controller
//! - **Type-safe register access**: Safe hardware register operations
//!
//! ## Platform Support
//!
//! Currently optimized for:
//! - Rockchip RK3568
//! - Rockchip RK3588
//! - Other platforms using DWCMSHC SDHCI controller
//!
//! ## Usage Example
//!
//! ```rust,no_run
//! use sdmmc::emmc::EMmcHost;
//!
//! // Create EMMC controller instance at hardware base address
//! let mut emmc = EMmcHost::new(0xfe2e0000);
//!
//! // Initialize the controller and card
//! match emmc.init() {
//!     Ok(_) => {
//!         println!("EMMC initialized successfully");
//!     }
//!     Err(e) => {
//!         println!("EMMC initialization failed: {:?}", e);
//!     }
//! }
//! ```
//!
//! ## Features
//!
//! - `pio` (default): Use PIO (Programmed I/O) for data transfers
//! - `dma`: Use DMA (Direct Memory Access) for data transfers
//!
//! ## Modules
//!
//! - [`emmc`]: Main EMMC controller implementation
//! - [`err`]: Error types and handling

#![no_std]
// #![no_main]
#![feature(alloc_error_handler)]

pub mod emmc;
pub mod err;

use log::warn;

pub const BLOCK_SIZE: usize = 512;

/// Dump memory region for debugging purposes
///
/// # Safety
///
/// This function is unsafe because it reads arbitrary memory addresses.
/// The caller must ensure that the memory region `addr..addr+size` is valid.
///
/// # Arguments
///
/// * `addr` - Starting address of the memory region
/// * `size` - Size of the memory region in bytes
pub unsafe fn dump_memory_region(addr: usize, size: usize) {
    let start_ptr = addr as *const u32;
    let word_count = size / 4; // 每个u32是4字节

    warn!(
        "Memory dump from 0x{:08x} to 0x{:08x}:",
        addr,
        addr + size - 1
    );

    for i in 0..word_count {
        if i % 4 == 0 {
            warn!("\n0x{:08x}:", addr + i * 4);
        }

        let value = unsafe { *start_ptr.add(i) };
        warn!(" 0x{:08x}", value);
    }

    warn!("");
}

/// Kernel trait for platform-specific delay implementation
///
/// Users of this library must implement this trait and register it using the `set_impl!` macro.
pub trait Kernel {
    /// Sleep for the specified number of microseconds
    fn sleep(us: u64);
}

/// Internal delay function using the registered Kernel implementation
pub(crate) fn delay_us(us: u64) {
    unsafe extern "Rust" {
        fn delay_us(us: u64);
    }

    unsafe {
        delay_us(us);
    }
}

/// Register a Kernel implementation for delay functions
///
/// # Usage
///
/// ```rust
/// struct MyKernel;
///
/// impl Kernel for MyKernel {
///     fn sleep(us: u64) {
///         // Implementation...
///     }
/// }
///
/// set_impl!(MyKernel);
/// ```
#[macro_export]
macro_rules! set_impl {
    ($t: ty) => {
        #[unsafe(no_mangle)]
        unsafe fn delay_us(us: u64) {
            <$t as $crate::Kernel>::sleep(us)
        }
    };
}
