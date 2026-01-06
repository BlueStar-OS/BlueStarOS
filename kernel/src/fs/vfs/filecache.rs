use alloc::{collections::btree_map::BTreeMap, sync::Arc};
use spin::Mutex;

use crate::{fs::vfs::File, memory::FramTracker};
pub struct FilePageNum(pub usize);
pub const FIELCACHE_MAX_COUNT:usize=100;
pub struct FileCache{
    cache:BTreeMap<Arc<dyn File>,FileFrameCache>
}

pub struct FileFrameCache{
    frame_cache:BTreeMap<FilePageNum,FileFrame>
}

/// 后面加页脏页可以去掉这个结构体的使用
pub struct FileFrame{
    frame:Arc<FramTracker>,
    dirty:bool
}


impl FileCache {
    // new
    //insert
    // read ->framenum
    // write -> framenum

    //auto mark,lru flush
}