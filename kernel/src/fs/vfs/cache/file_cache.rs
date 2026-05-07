//! 以PAGE_SIEZ为页面大小进行缓存和写入

use alloc::{sync::Arc, vec::Vec};
use spin::Mutex;

use crate::fs::vfs::File;

pub struct CACHEKEY {
    fs: Arc<Mutex<dyn File>>, // 需要互斥？，多个线程同时用这个刷新就需要,我们直接使用对应块设备来写
    start_lbn: usize,
    len: usize,
    data: Vec<u8>,
}
pub struct FILECACHE {
    // cache:BTreeMap<>
}
