//! Doom/图形用户态当前需要的最小设备文件集合。
//!
//! 第一版先提供三个能力：
//! 1. `/dev/fb0`：把已经 modeset 完成的 VGA framebuffer 暴露为可读写文件；
//! 2. `/dev/keyboard`：暴露当前 UART 键盘输入字节流；
//! 3. `ioctl`：提供用户态查询 framebuffer 基本几何信息，以及切换键盘非阻塞模式。
//!
//! 设计备注：
//! - Linux 正式 framebuffer ABI 是 `FBIOGET_VSCREENINFO` / `FBIOGET_FSCREENINFO`
//!   等复杂 ioctl，定义见 Linux 5.4.29 `include/uapi/linux/fb.h:12-37`。
//! - 本项目这一版只对接 Doomgeneric 的极小子集，所以采用更简单的本地命令号，
//!   但仍保留 `ioctl` 入口形式，后续可平滑升级到 Linux 风格 ABI。

use crate::driver::gpu::vga_console::VgaScreen;
use crate::fs::component::tty::Stdin;
use crate::fs::vfs::{File, VfsFsError, VfsStat, VFS_DT_REG};
use alloc::sync::Arc;
use core::cmp::min;
use core::convert::TryFrom;
use core::sync::atomic::{AtomicBool, Ordering};
use log::warn;
use spin::Mutex;

/// Doomgeneric `doomgeneric_soso.c` 使用的最小 framebuffer ioctl 命令号。
///
/// 对应文件：
/// - `/home/inkbottle/othersrc/doomgeneric/doomgeneric/doomgeneric_soso.c:28-32`
#[repr(u32)]
enum FrameBufferIoctl {
    GetWidth = 0,
    GetHeight = 1,
    GetBitsPerPixel = 2,
}

impl TryFrom<u32> for FrameBufferIoctl {
    type Error = VfsFsError;

    fn try_from(raw_cmd: u32) -> Result<Self, Self::Error> {
        match raw_cmd {
            0 => Ok(Self::GetWidth),
            1 => Ok(Self::GetHeight),
            2 => Ok(Self::GetBitsPerPixel),
            _ => Err(VfsFsError::NotSupported),
        }
    }
}

/// 键盘设备当前支持的最小 ioctl 命令集合。
///
/// 命令 1 同样对齐 Doomgeneric `doomgeneric_soso.c:150-151` 的调用方式：
/// `ioctl(KeyboardFd, 1, (void*)1)`.
#[repr(u32)]
enum KeyboardIoctl {
    SetNonBlocking = 1,
}

impl TryFrom<u32> for KeyboardIoctl {
    type Error = VfsFsError;

    fn try_from(raw_cmd: u32) -> Result<Self, Self::Error> {
        match raw_cmd {
            1 => Ok(Self::SetNonBlocking),
            _ => Err(VfsFsError::NotSupported),
        }
    }
}

/// framebuffer 顺序读写光标。
///
/// 这里用 newtype 包装，避免把“当前文件偏移”与 framebuffer 大小、像素索引等
/// 其他 `usize` 混在一起。
#[derive(Clone, Copy, Debug, Default)]
struct FrameBufferOffset(usize);

/// `/dev/fb0` 设备文件。
///
/// TODO(dirinkbottle):
/// 当前 ramfs 的设备节点 open 后会复用同一个 `Arc<FrameBufferDeviceFile>`，
/// 因而 `file_offset` 是“跨打开实例共享”的。后面如果你要把 `/dev/fb0`
/// 当成通用文件接口长期使用，建议给 ramfs 设备节点加一个“每次 open 返回独立 handle”
/// 的包装层，把 offset 放到 handle 里。
pub struct FrameBufferDeviceFile {
    file_offset: Mutex<FrameBufferOffset>,
}

/// `/dev/keyboard` 设备文件。
///
/// TODO(dirinkbottle):
/// 当前返回的是 UART 输入字节流，不是 Linux evdev，也不是 PC AT scancode。
/// 如果后面要和 Doom/SDL/输入子系统更严谨对接，需要单独设计按键事件层。
pub struct KeyboardDeviceFile {
    non_blocking: AtomicBool,
}

/// 统一读取当前 VGA framebuffer 视图。
fn with_framebuffer<T>(
    callback: impl FnOnce(*mut u8, usize, usize, usize, usize) -> Result<T, VfsFsError>,
) -> Result<T, VfsFsError> {
    unsafe {
        if VgaScreen.fb_base.is_null() || VgaScreen.width == 0 || VgaScreen.height == 0 {
            return Err(VfsFsError::NoDevice);
        }

        // 当前 bochs modeset 目标固定为 32bpp，和 Linux bochs 驱动里常见的
        // XRGB8888 路径一致；参考 Linux 5.4.29
        // `drivers/gpu/drm/bochs/bochs_hw.c:141-154,207-235`。
        let bits_per_pixel = 32usize;
        let bytes_per_pixel = bits_per_pixel / 8;
        let framebuffer_bytes = VgaScreen
            .width
            .saturating_mul(VgaScreen.height)
            .saturating_mul(bytes_per_pixel);

        callback(
            VgaScreen.fb_base as *mut u8,
            framebuffer_bytes,
            VgaScreen.width,
            VgaScreen.height,
            bits_per_pixel,
        )
    }
}

impl FrameBufferDeviceFile {
    /// 创建 `/dev/fb0` 设备对象。
    pub fn new() -> Self {
        Self {
            file_offset: Mutex::new(FrameBufferOffset::default()),
        }
    }

    /// 复制 framebuffer 的一段字节到普通内存缓冲区。
    fn read_bytes_from_framebuffer(
        &self,
        offset_bytes: usize,
        user_buffer: &mut [u8],
    ) -> Result<usize, VfsFsError> {
        with_framebuffer(|framebuffer_base, framebuffer_len, _, _, _| {
            if offset_bytes >= framebuffer_len {
                return Ok(0);
            }

            let readable_bytes = min(user_buffer.len(), framebuffer_len - offset_bytes);
            let framebuffer_slice = unsafe {
                core::slice::from_raw_parts(framebuffer_base.add(offset_bytes), readable_bytes)
            };
            user_buffer[..readable_bytes].copy_from_slice(framebuffer_slice);
            Ok(readable_bytes)
        })
    }

    /// 把普通内存缓冲区的一段字节写入 framebuffer。
    ///
    /// Linux 在 shadow-buffer fbdev helper 路径中，会在 dirty 区域提交时做成块拷贝；
    /// 参考 Linux 5.4.29 `drivers/gpu/drm/drm_fb_helper.c:381-420`。
    /// 这里先采用最直接的内存拷贝版本，后续若做双缓冲/dirty rect，再往上演进。
    fn write_bytes_to_framebuffer(
        &self,
        offset_bytes: usize,
        user_buffer: &[u8],
    ) -> Result<usize, VfsFsError> {
        with_framebuffer(|framebuffer_base, framebuffer_len, _, _, _| {
            if offset_bytes >= framebuffer_len {
                return Ok(0);
            }

            let writable_bytes = min(user_buffer.len(), framebuffer_len - offset_bytes);
            let framebuffer_slice = unsafe {
                core::slice::from_raw_parts_mut(framebuffer_base.add(offset_bytes), writable_bytes)
            };
            framebuffer_slice.copy_from_slice(&user_buffer[..writable_bytes]);
            Ok(writable_bytes)
        })
    }
}

impl File for FrameBufferDeviceFile {
    fn read(&self, user_buffer: &mut [u8]) -> Result<usize, VfsFsError> {
        let current_offset = self.file_offset.lock().0;
        let read_bytes = self.read_bytes_from_framebuffer(current_offset, user_buffer)?;
        self.file_offset.lock().0 = current_offset.saturating_add(read_bytes);
        Ok(read_bytes)
    }

    fn write(&self, user_buffer: &[u8]) -> Result<usize, VfsFsError> {
        let current_offset = self.file_offset.lock().0;
        let written_bytes = self.write_bytes_to_framebuffer(current_offset, user_buffer)?;
        self.file_offset.lock().0 = current_offset.saturating_add(written_bytes);
        Ok(written_bytes)
    }

    fn read_at(&self, offset: usize, user_buffer: &mut [u8]) -> Result<usize, VfsFsError> {
        self.read_bytes_from_framebuffer(offset, user_buffer)
    }

    fn write_at(&self, offset: usize, user_buffer: &[u8]) -> Result<usize, VfsFsError> {
        self.write_bytes_to_framebuffer(offset, user_buffer)
    }

    fn lseek(&self, offset: isize, whence: usize) -> Result<usize, VfsFsError> {
        let framebuffer_len = with_framebuffer(|_, framebuffer_len, _, _, _| Ok(framebuffer_len))?;
        let current_offset = self.file_offset.lock().0 as isize;
        let next_offset = match whence {
            0 => offset,
            1 => current_offset.saturating_add(offset),
            2 => framebuffer_len as isize + offset,
            _ => return Err(VfsFsError::Invalid),
        };

        if next_offset < 0 {
            return Err(VfsFsError::Invalid);
        }

        *self.file_offset.lock() = FrameBufferOffset(next_offset as usize);
        Ok(next_offset as usize)
    }

    fn stat(&self) -> Result<VfsStat, VfsFsError> {
        with_framebuffer(|_, framebuffer_len, _, _, _| {
            Ok(VfsStat {
                inode: 0,
                size: framebuffer_len as u64,
                mode: 0,
                file_type: VFS_DT_REG,
            })
        })
    }

    fn ioctl(&self, cmd: u32, _arg: usize) -> Result<usize, VfsFsError> {
        let framebuffer_ioctl = FrameBufferIoctl::try_from(cmd)?;
        with_framebuffer(|_, _, width_pixels, height_pixels, bits_per_pixel| {
            let ioctl_result = match framebuffer_ioctl {
                FrameBufferIoctl::GetWidth => width_pixels,
                FrameBufferIoctl::GetHeight => height_pixels,
                FrameBufferIoctl::GetBitsPerPixel => bits_per_pixel,
            };
            Ok(ioctl_result)
        })
    }
}

impl KeyboardDeviceFile {
    /// 创建 `/dev/keyboard` 设备对象。
    pub fn new() -> Self {
        Self {
            non_blocking: AtomicBool::new(false),
        }
    }

    /// 从当前键盘输入缓冲区尽量多地取字节。
    fn drain_available_input(&self, user_buffer: &mut [u8]) -> usize {
        let mut copied_bytes = 0usize;

        for output_slot in user_buffer.iter_mut() {
            let Some(input_byte) = crate::arch::driver::keyboard::read_input() else {
                break;
            };
            *output_slot = input_byte;
            copied_bytes += 1;
        }

        copied_bytes
    }
}

impl File for KeyboardDeviceFile {
    fn read(&self, user_buffer: &mut [u8]) -> Result<usize, VfsFsError> {
        if user_buffer.is_empty() {
            return Ok(0);
        }

        let immediately_available = self.drain_available_input(user_buffer);
        if immediately_available > 0 {
            return Ok(immediately_available);
        }

        if self.non_blocking.load(Ordering::Relaxed) {
            // TODO(dirinkbottle):
            // 更标准的 tty/non-blocking 语义应该返回 -EAGAIN，Linux 在
            // `drivers/tty/n_tty.c:2152-2155` 也是这么做的。
            // 但你现在的 `sys_read` 还没有细粒度错误映射，这里先返回 0，
            // 保证 Doomgeneric 的 `if (read(...) > 0)` 路径能直接工作。
            return Ok(0);
        }

        user_buffer[0] = Stdin::get_char();
        let drain_tail = self.drain_available_input(&mut user_buffer[1..]);
        Ok(1 + drain_tail)
    }

    fn write(&self, _user_buffer: &[u8]) -> Result<usize, VfsFsError> {
        Err(VfsFsError::NotSupported)
    }

    fn stat(&self) -> Result<VfsStat, VfsFsError> {
        Ok(VfsStat {
            inode: 0,
            size: 0,
            mode: 0,
            file_type: VFS_DT_REG,
        })
    }

    fn ioctl(&self, cmd: u32, arg: usize) -> Result<usize, VfsFsError> {
        match KeyboardIoctl::try_from(cmd)? {
            KeyboardIoctl::SetNonBlocking => {
                let enable_non_blocking = arg != 0;
                self.non_blocking
                    .store(enable_non_blocking, Ordering::Relaxed);
                warn!(
                    "[keyboard-dev] non-blocking mode {}",
                    if enable_non_blocking {
                        "enabled"
                    } else {
                        "disabled"
                    }
                );
                Ok(0)
            }
        }
    }
}

/// 构造 framebuffer 设备文件节点。
pub fn framebuffer_device_file() -> Arc<dyn File> {
    Arc::new(FrameBufferDeviceFile::new())
}

/// 构造键盘设备文件节点。
pub fn keyboard_device_file() -> Arc<dyn File> {
    Arc::new(KeyboardDeviceFile::new())
}
