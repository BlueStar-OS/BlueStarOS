use core::fmt::{self, Write};
use alloc::boxed::Box;
use alloc::sync::Arc;
use crate::{sbi, task::TASK_MANAER};
use crate::fs::vfs::{FileDescriptor, FileDescriptorTrait, OpenFlags, VfsFsError};


pub const FD_TYPE_STDIN: usize = 0;
pub const FD_TYPE_STDOUT: usize = 1;
pub const FD_TYPE_STDERR: usize = 2;

/// 标准输出文件节点
pub struct Stdout;

/// 标准输入文件节点
pub struct Stdin;

///标准错误文件节点
pub struct Stderr;


impl Stdin {
    ///调用栈顶必须为traphandler！！！，因为其中有TASK_MANAER.suspend_and_run_task();
    pub fn get_char() -> u8 {
        //直接调用sbi接口，返回一个字符，没有字符就挂起
        let cha = sbi::get_char() as u8;

        if cha == 0 {
            TASK_MANAER.suspend_and_run_task();//没有字符就切换任务
        }
        
        cha
    }
}

impl FileDescriptorTrait for Stdout {
    fn read(&mut self, _buf: &mut [u8]) -> Result<usize, VfsFsError> {
        Err(VfsFsError::NotSupported)
    }

    fn write(&mut self, buf: &[u8]) -> Result<usize, VfsFsError> {
        for &byte in buf {
            sbi::putc(byte as usize);
        }
        Ok(buf.len())
    }
}

impl FileDescriptorTrait for Stdin {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, VfsFsError> {
        let mut read_count = 0usize;
        buf.iter_mut().for_each(|b| *b = 0);

        for slot in buf.iter_mut() {
            let mut cha = sbi::get_char();
            while cha == 0 {
                TASK_MANAER.suspend_and_run_task();
                cha = sbi::get_char();
            }
            *slot = cha as u8;
            read_count += 1;
            if *slot == 13 {
                break;
            }
        }
        Ok(read_count)
    }

    fn write(&mut self, _buf: &[u8]) -> Result<usize, VfsFsError> {
        Err(VfsFsError::NotSupported)
    }
}

impl FileDescriptorTrait for Stderr {
    fn read(&mut self, _buf: &mut [u8]) -> Result<usize, VfsFsError> {
        Err(VfsFsError::NotSupported)
    }

    fn write(&mut self, buf: &[u8]) -> Result<usize, VfsFsError> {
        for &b in b"<3>" {
            sbi::putc(b as usize);
        }
        for &byte in buf {
            sbi::putc(byte as usize);
        }
        Ok(buf.len())
    }
}

impl Write for Stdout {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for cha in s.chars() {
            sbi::putc(cha as usize);
        }
        Ok(())
    }
}

/// 打印函数
pub fn print(fmt: fmt::Arguments) {
    let mut stdout = Stdout;
    stdout.write_fmt(fmt).unwrap()
}

pub fn stdin_fd() -> Arc<FileDescriptor> {
    Arc::new(FileDescriptor::new_from_inner(OpenFlags::RDONLY, false, Box::new(Stdin)))
}

pub fn stdout_fd() -> Arc<FileDescriptor> {
    Arc::new(FileDescriptor::new_from_inner(OpenFlags::WRONLY, true, Box::new(Stdout)))
}

pub fn stderr_fd() -> Arc<FileDescriptor> {
    Arc::new(FileDescriptor::new_from_inner(OpenFlags::WRONLY, true, Box::new(Stderr)))
}
