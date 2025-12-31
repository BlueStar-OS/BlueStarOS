use core::fmt::{self, Write};
use alloc::{collections::vec_deque::VecDeque, sync::Arc, vec::Vec};
use spin::mutex::Mutex;
use lazy_static::lazy_static;
use crate::{sys_write};

pub const FD_TYPE_STDIN: usize = 0;
pub const FD_TYPE_STDOUT: usize = 1;
const BUFFER_SIZE: usize = 256 * 10;

struct STDIN;
struct STDOUT;
pub struct StdinBuffer{
   pub buffer:Vec<u8>
}

struct STDBUFFER(VecDeque<u8>);
// 使用 static 和 Mutex，避免 lazy_static 的初始化死锁
lazy_static!{
 static ref STD_BUFFER: Arc<Mutex<STDBUFFER>> =Arc::new(Mutex::new(STDBUFFER(VecDeque::new())));

}
impl STDBUFFER {
    fn flush(&mut self) -> isize {
        let buffer = self.0.make_contiguous();
        let resultcode =sys_write(FD_TYPE_STDOUT, buffer.as_ptr() as usize, buffer.len());
        self.0.clear();
        resultcode as isize
    }
}

impl StdinBuffer {
    pub fn new(len:usize)->Self{
       StdinBuffer{
        buffer:Vec::with_capacity(len)
       }
    }
}




impl Write for STDBUFFER {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for byte in s.as_bytes().iter() {
            self.0.push_back(*byte);
            if self.0.len() == BUFFER_SIZE || *byte==b'\n'{
                if self.flush()==-1{
                    return Err(core::fmt::Error);
                }
            }
        }
        Ok(())
    }
}


///手动刷新buffer，用于处理不能达到自动flush的情况
pub fn stdout_buffer_flush(){
    //获取锁
    if let Some(mut locak) = STD_BUFFER.try_lock(){
        locak.flush();
        drop(locak);
    }
    // 如果当前持锁（比如 print 内部），这里直接返回，避免递归加锁导致 panic
    
}

pub fn print(fmt: fmt::Arguments,flush:bool) {
    // 不使用 expect()，避免在 panic handler 中再次获取锁导致死锁
    if let Some(mut buffer) = STD_BUFFER.try_lock(){
        let _ = buffer.write_fmt(fmt);
        if flush {
            let _ = buffer.flush();
        }
        drop(buffer);
    }
}

#[macro_export]
macro_rules! print {
    ($lit:literal $(,$($arg:tt)+)?) => {
        //手动flush，不然缓冲区不满，没有\n不会输出
        crate::print(format_args!($lit $(,$($arg)+)?),true)
    };
}


#[macro_export]
macro_rules! println {
    ($lit:literal $(,$($arg:tt)+)?) => {
        crate::print(format_args!(concat!($lit,'\n') $(,$($arg)+)?),false)
    };
}