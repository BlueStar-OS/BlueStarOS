use alloc::vec::Vec;
use core::{
    alloc::Layout,
    ptr::{slice_from_raw_parts_mut, NonNull},
};

use crate::{flush, map, unmap, Direction};

pub mod r#box;
pub mod vec;

struct DCommon<T> {
    addr: NonNull<T>,
    bus_addr: u64,
    layout: Layout,
    direction: Direction,
}

impl<T> DCommon<T> {
    pub fn zeros(layout: Layout, direction: Direction) -> Option<Self> {
        unsafe {
            let mut addr = NonNull::new(crate::alloc(layout))?;
            (*slice_from_raw_parts_mut(addr.as_mut(), layout.size())).fill(0);

            let bus_addr = map(addr, layout.size(), direction);
            flush(addr, layout.size());
            Some(Self {
                bus_addr,
                addr: addr.cast(),
                layout,
                direction,
            })
        }
    }

    pub fn from_vec(mut value: Vec<T>, direction: Direction) -> Self {
        unsafe {
            let layout = Layout::from_size_align_unchecked(
                value.capacity() * size_of::<T>(),
                align_of::<T>(),
            );

            let addr = NonNull::new(value.as_mut_ptr()).unwrap();

            core::mem::forget(value);

            let bus_addr = map(addr.cast(), layout.size(), direction);
            flush(addr.cast(), layout.size());
            Self {
                bus_addr,
                addr: addr.cast(),
                layout,
                direction,
            }
        }
    }

    pub fn preper_read(&self, ptr: NonNull<u8>, size: usize) {
        self.direction.preper_read(ptr, size);
    }

    pub fn confirm_write(&self, ptr: NonNull<u8>, size: usize) {
        self.direction.confirm_write(ptr, size);
    }
}

impl<T> Drop for DCommon<T> {
    fn drop(&mut self) {
        if self.layout.size() > 0 {
            unmap(self.addr.cast(), self.layout.size());

            unsafe { crate::dealloc(self.addr.as_ptr() as _, self.layout) };
        }
    }
}
