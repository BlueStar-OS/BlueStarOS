use alloc::sync::{Arc, Weak};

use crate::sync::UPSafeCell;
use crate::fs::vfs::{FileDescriptorTrait, VfsFsError};

pub const RINGBUFFERSIZE:usize = 512;

///pipe模型
pub struct Pipe{
    readble:bool,
    writeble:bool,
    ringbuffer:Arc<UPSafeCell<PipeRingBuffer>>,
}

///pipe环形缓冲区模型
pub struct PipeRingBuffer{
    buffer:[u8;RINGBUFFERSIZE],
    status:PipeRingBufferStatus,
    head:usize,
    tail:usize,
    write_point:Weak<UPSafeCell<Pipe>>, //写段弱引用计数,检测写段是否关闭
}

///pipe环形缓冲区状态
pub enum PipeRingBufferStatus {
    Empty,
    Full,
    Normal,
}

impl PipeRingBuffer {
    pub fn new() -> Self {
        Self {
            buffer: [0u8; RINGBUFFERSIZE],
            status: PipeRingBufferStatus::Empty,
            head: 0,
            tail: 0,
            write_point: Weak::new(),
        }
    }

    pub fn set_write_point(&mut self, w: Weak<UPSafeCell<Pipe>>) {
        self.write_point = w;
    }

    fn is_write_end_closed(&self) -> bool {
        self.write_point.upgrade().is_none()
    }

    fn readable_len(&self) -> usize {
        match self.status {
            PipeRingBufferStatus::Empty => 0,
            PipeRingBufferStatus::Full => RINGBUFFERSIZE,
            PipeRingBufferStatus::Normal => {
                if self.tail >= self.head {
                    self.tail - self.head
                } else {
                    RINGBUFFERSIZE - self.head + self.tail
                }
            }
        }
    }

    fn writable_len(&self) -> usize {
        RINGBUFFERSIZE - self.readable_len()
    }

    fn push_byte(&mut self, b: u8) -> bool {
        if matches!(self.status, PipeRingBufferStatus::Full) {
            return false;
        }
        self.buffer[self.tail] = b;
        self.tail = (self.tail + 1) % RINGBUFFERSIZE;
        if self.tail == self.head {
            self.status = PipeRingBufferStatus::Full;
        } else {
            self.status = PipeRingBufferStatus::Normal;
        }
        true
    }

    fn pop_byte(&mut self) -> Option<u8> {
        if matches!(self.status, PipeRingBufferStatus::Empty) {
            return None;
        }
        let b = self.buffer[self.head];
        self.head = (self.head + 1) % RINGBUFFERSIZE;
        if self.head == self.tail {
            self.status = PipeRingBufferStatus::Empty;
        } else {
            self.status = PipeRingBufferStatus::Normal;
        }
        Some(b)
    }

    pub fn write(&mut self, buf: &[u8]) -> usize {
        if buf.is_empty() {
            return 0;
        }
        let mut n = 0usize;
        for &b in buf.iter() {
            if !self.push_byte(b) {
                break;
            }
            n += 1;
        }
        n
    }

    pub fn read(&mut self, buf: &mut [u8]) -> usize {
        if buf.is_empty() {
            return 0;
        }
        let mut n = 0usize;
        for slot in buf.iter_mut() {
            match self.pop_byte() {
                Some(b) => {
                    *slot = b;
                    n += 1;
                }
                None => break,
            }
        }
        n
    }

    pub fn can_read(&self) -> bool {
        self.readable_len() != 0
    }

    pub fn can_write(&self) -> bool {
        self.writable_len() != 0
    }
}

impl Pipe {
    pub fn new(readble: bool, writeble: bool, ringbuffer: Arc<UPSafeCell<PipeRingBuffer>>) -> Self {
        Self {
            readble,
            writeble,
            ringbuffer,
        }
    }

    pub fn read(&self, buf: &mut [u8]) -> usize {
        if !self.readble {
            return 0;
        }
        let mut ring = self.ringbuffer.lock();
        if !ring.can_read() {
            if ring.is_write_end_closed() {
                return 0;
            }
            return 0;
        }
        ring.read(buf)
    }

    pub fn write(&self, buf: &[u8]) -> usize {
        if !self.writeble {
            return 0;
        }
        let mut ring = self.ringbuffer.lock();
        if !ring.can_write() {
            return 0;
        }
        ring.write(buf)
    }
}

pub fn make_pipe() -> (Arc<UPSafeCell<Pipe>>, Arc<UPSafeCell<Pipe>>) {
    let ring = Arc::new(UPSafeCell::new(PipeRingBuffer::new()));
    let read_end = Arc::new(UPSafeCell::new(Pipe::new(true, false, ring.clone())));
    let write_end = Arc::new(UPSafeCell::new(Pipe::new(false, true, ring.clone())));
    ring.lock().set_write_point(Arc::downgrade(&write_end));
    (read_end, write_end)
}

pub struct PipeHandle {
    end: Arc<UPSafeCell<Pipe>>,
}

impl PipeHandle {
    pub fn new(end: Arc<UPSafeCell<Pipe>>) -> Self {
        Self { end }
    }
}

impl FileDescriptorTrait for PipeHandle {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, VfsFsError> {
        let pipe = self.end.lock();
        Ok(pipe.read(buf))
    }

    fn write(&mut self, buf: &[u8]) -> Result<usize, VfsFsError> {
        let pipe = self.end.lock();
        Ok(pipe.write(buf))
    }
}
