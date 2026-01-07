#[cfg(feature = "ext4")]
pub mod ext4_backend;
pub mod ramfs;
pub mod fat32;
use alloc::sync::Arc;
#[cfg(feature = "ext4")]
pub use ext4_backend::*;
pub use ramfs::*;
use spin::Mutex;

use crate::{driver::VirtBlk, fs::vfs::{MountFs, VfsFs}};