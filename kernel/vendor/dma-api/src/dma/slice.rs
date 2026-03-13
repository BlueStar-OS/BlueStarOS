use core::{
    marker::PhantomData,
    mem::{size_of, size_of_val},
    ops::{Deref, Index},
    ptr::{slice_from_raw_parts, NonNull},
};

use crate::{flush, map, unmap, Direction};

#[repr(transparent)]
pub struct DSlice<'a, T> {
    inner: DSliceCommon<'a, T>,
}

impl<T> DSlice<'_, T> {
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn bus_addr(&self) -> u64 {
        self.inner.bus_addr
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<'a, T> From<&'a [T]> for DSlice<'a, T> {
    fn from(value: &'a [T]) -> Self {
        Self {
            inner: DSliceCommon::new(value, Direction::ToDevice),
        }
    }
}

impl<T> Index<usize> for DSlice<'_, T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        self.inner.index(index)
    }
}

impl<T> Deref for DSlice<'_, T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[repr(transparent)]
pub struct DSliceMut<'a, T> {
    inner: DSliceCommon<'a, T>,
}

impl<'a, T> DSliceMut<'a, T> {
    pub fn from(value: &'a mut [T], direction: Direction) -> Self {
        Self {
            inner: DSliceCommon::new(value, direction),
        }
    }

    pub fn bus_addr(&self) -> u64 {
        self.inner.bus_addr
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn set(&self, index: usize, value: T) {
        assert!(index < self.len());

        unsafe {
            let ptr = self.inner.addr.add(index);

            ptr.write_volatile(value);

            self.inner
                .direction
                .confirm_write(ptr.cast(), size_of::<T>());
        }
    }

    pub fn preper_read_all(&self) {
        self.inner.preper_read_all();
    }

    pub fn preper_write_all(&self) {
        self.inner
            .direction
            .confirm_write(self.inner.addr.cast(), self.inner.size);
    }
}

impl<T> Index<usize> for DSliceMut<'_, T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        self.inner.index(index)
    }
}

impl<T> Deref for DSliceMut<'_, T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

struct DSliceCommon<'a, T> {
    addr: NonNull<T>,
    size: usize,
    bus_addr: u64,
    direction: Direction,
    _marker: PhantomData<&'a T>,
}

impl<'a, T> DSliceCommon<'a, T> {
    fn new(s: &'a [T], direction: Direction) -> Self {
        let size = size_of_val(s);
        let ptr = unsafe { NonNull::new_unchecked(s.as_ptr() as usize as *mut T) };
        let bus_addr = map(ptr.cast(), size, direction);

        flush(ptr.cast(), size);

        Self {
            addr: ptr,
            size,
            bus_addr,
            direction,
            _marker: PhantomData,
        }
    }

    fn len(&self) -> usize {
        self.size / size_of::<T>()
    }

    fn index(&self, index: usize) -> &T {
        assert!(index < self.len());

        let ptr = unsafe { self.addr.add(index) };

        self.direction.preper_read(ptr.cast(), size_of::<T>());

        unsafe { ptr.as_ref() }
    }

    fn preper_read_all(&self) {
        self.direction.preper_read(self.addr.cast(), self.size);
    }
}

impl<T> Drop for DSliceCommon<'_, T> {
    fn drop(&mut self) {
        unmap(self.addr.cast(), self.size);
    }
}

impl<T> Deref for DSliceCommon<'_, T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        self.direction.preper_read(self.addr.cast(), self.size);
        unsafe { &*slice_from_raw_parts(self.addr.as_ptr(), self.len()) }
    }
}
