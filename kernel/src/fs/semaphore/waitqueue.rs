use alloc::{sync::Arc, vec::Vec};

use crate::{sync::UPSafeCell, task::TaskControlBlock};

pub struct WaitQueue{
    task:UPSafeCell<Vec<Arc<TaskControlBlock>>>  
}