use core::cell::{RefCell};



//确保在单核环境中的数据安全共享访问
pub struct UPSafeCell<T>{
    inner:RefCell<T>
}

unsafe impl<T> Sync  for UPSafeCell<T>{}
unsafe impl<T> Send for UPSafeCell<T> {}

impl<T> UPSafeCell<T>{
    pub const fn new(value:T)->Self{
        UPSafeCell{
            inner:RefCell::new(value)
        }
    }

    #[track_caller]
    pub fn lock(&self)->core::cell::RefMut<'_,T>{
        match self.inner.try_borrow_mut() {
            Ok(g) => g,
            Err(_) => {
                let loc = core::panic::Location::caller();
                panic!(
                    "RefCell already borrowed at {}:{}:{}",
                    loc.file(),
                    loc.line(),
                    loc.column()
                );
            }
        }
    }
}