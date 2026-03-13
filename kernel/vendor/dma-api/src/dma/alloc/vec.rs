#[cfg(feature = "alloc")]
use alloc::vec::Vec;
use core::{
    alloc::Layout,
    mem::size_of,
    ops::{Deref, Index},
};

use super::DCommon;
use crate::Direction;

pub struct DVec<T> {
    inner: DCommon<T>,
}

impl<T> DVec<T> {
    const T_SIZE: usize = size_of::<T>();

    pub fn zeros(len: usize, align: usize, direction: Direction) -> Option<Self> {
        let size = len * size_of::<T>();
        let layout = Layout::from_size_align(size, align).unwrap();

        Some(Self {
            inner: DCommon::zeros(layout, direction)?,
        })
    }

    #[cfg(feature = "alloc")]
    pub fn from_vec(value: Vec<T>, direction: Direction) -> Self {
        Self {
            inner: DCommon::from_vec(value, direction),
        }
    }

    #[cfg(feature = "alloc")]
    pub fn to_vec(mut self) -> Vec<T> {
        unsafe {
            self.inner
                .preper_read(self.inner.addr.cast(), self.inner.layout.size());
            crate::unmap(self.inner.addr.cast(), self.inner.layout.size());
            let len = self.len();

            self.inner.layout = Layout::from_size_align_unchecked(0, 0x1000);
            Vec::from_raw_parts(self.inner.addr.as_ptr(), len, len)
        }
    }

    pub fn len(&self) -> usize {
        self.inner.layout.size() / size_of::<T>()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn bus_addr(&self) -> u64 {
        self.inner.bus_addr
    }

    pub fn get(&self, index: usize) -> Option<T> {
        if index >= self.len() {
            return None;
        }

        unsafe {
            let ptr = self.inner.addr.add(index);

            self.inner.preper_read(ptr.cast(), Self::T_SIZE);

            Some(ptr.read_volatile())
        }
    }

    pub fn set(&mut self, index: usize, value: T) {
        assert!(
            index < self.len(),
            "index out of range, index: {},len: {}",
            index,
            self.len()
        );

        unsafe {
            let ptr = self.inner.addr.add(index);

            ptr.write_volatile(value);

            self.inner.confirm_write(ptr.cast(), Self::T_SIZE);
        }
    }

    fn as_slice_mut(&mut self) -> &mut [T] {
        unsafe { core::slice::from_raw_parts_mut(self.inner.addr.as_ptr(), self.len()) }
    }
}

impl<T> Index<usize> for DVec<T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        assert!(index < self.len());

        let ptr = unsafe { self.inner.addr.add(index) };

        self.inner.preper_read(ptr.cast(), Self::T_SIZE);

        unsafe { &*ptr.as_ptr() }
    }
}

impl<T: Copy> DVec<T> {
    pub fn copy_from_slice(&mut self, src: &[T]) {
        assert!(src.len() <= self.len());

        self.as_slice_mut().copy_from_slice(src);

        self.inner
            .confirm_write(self.inner.addr.cast(), Self::T_SIZE * src.len());
    }
}

impl<T> Deref for DVec<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        self.inner
            .preper_read(self.inner.addr.cast(), Self::T_SIZE * self.len());
        unsafe { core::slice::from_raw_parts(self.inner.addr.as_ptr(), self.len()) }
    }
}
