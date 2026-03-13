use core::alloc::Layout;

use crate::Direction;

use super::DCommon;

pub struct DBox<T> {
    inner: DCommon<T>,
}

impl<T> DBox<T> {
    const SIZE: usize = core::mem::size_of::<T>();

    pub fn zero_with_align(direction: Direction, align: usize) -> Option<Self> {
        let layout = Layout::from_size_align(Self::SIZE, align).ok()?;

        Some(Self {
            inner: DCommon::zeros(layout, direction)?,
        })
    }

    pub fn zero(direction: Direction) -> Option<Self> {
        let layout = Layout::new::<T>();
        Some(Self {
            inner: DCommon::zeros(layout, direction)?,
        })
    }
    pub fn bus_addr(&self) -> u64 {
        self.inner.bus_addr
    }

    pub fn read(&self) -> T {
        unsafe {
            let ptr = self.inner.addr;

            self.inner.preper_read(ptr.cast(), Self::SIZE);

            ptr.read_volatile()
        }
    }

    pub fn write(&mut self, value: T) {
        unsafe {
            let ptr = self.inner.addr;

            ptr.write_volatile(value);

            self.inner.confirm_write(ptr.cast(), Self::SIZE);
        }
    }

    pub fn modify(&mut self, f: impl FnOnce(&mut T)) {
        unsafe {
            let mut ptr = self.inner.addr;

            self.inner.preper_read(ptr.cast(), Self::SIZE);

            f(ptr.as_mut());

            self.inner.confirm_write(ptr.cast(), Self::SIZE);
        }
    }
}
