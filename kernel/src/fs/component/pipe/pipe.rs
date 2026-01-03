use alloc::sync::{Arc, Weak};

use crate::sync::UPSafeCell;
use crate::fs::vfs::{FileDescriptorTrait, VfsFsError};
use crate::task::TASK_MANAER;

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
    read_point:Weak<UPSafeCell<Pipe>>, //读端弱引用计数，检测读端是否关闭
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
            read_point:Weak::new(),
        }
    }

    pub fn set_write_point(&mut self, w: Weak<UPSafeCell<Pipe>>) {
        self.write_point = w;
    }
    pub fn set_read_point(&mut self, r: Weak<UPSafeCell<Pipe>>) {
        self.read_point = r;
    }

    fn is_write_end_closed(&self) -> bool {
        self.write_point.upgrade().is_none()
    }

    fn is_read_end_closed(&self) -> bool {
        self.read_point.upgrade().is_none()
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

    pub fn write(&mut self, buf: &[u8]) -> Result<usize, VfsFsError> {
        if buf.is_empty() {
            return Ok(0);
        }
        if self.is_read_end_closed() {
            return Err(VfsFsError::BrokenPipe);
        }
        let mut n = 0usize;
        for &b in buf.iter() {
            if !self.push_byte(b) {
                break;
            }
            n += 1;
        }
        Ok(n)
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

    pub fn read(&self, buf: &mut [u8]) -> Result<usize, VfsFsError> {
        if !self.readble {
            return Ok(0);
        }
        loop {
            let (can_read, write_closed) = {
                let ring = self.ringbuffer.lock();
                (ring.can_read(), ring.is_write_end_closed())
            };
            if can_read {
                let mut ring = self.ringbuffer.lock();
                return Ok(ring.read(buf));
            }
            if write_closed {
                return Ok(0);
            }
            TASK_MANAER.suspend_and_run_task();
        }
    }

    pub fn write(&self, buf: &[u8]) -> Result<usize, VfsFsError> {
        if !self.writeble {
            return Ok(0);
        }
        let mut ring = self.ringbuffer.lock();
        if !ring.can_write() {
            return Ok(0);
        }
        ring.write(buf)
    }
}

pub fn make_pipe() -> (Arc<UPSafeCell<Pipe>>, Arc<UPSafeCell<Pipe>>) {
    let ring = Arc::new(UPSafeCell::new(PipeRingBuffer::new()));
    let read_end = Arc::new(UPSafeCell::new(Pipe::new(true, false, ring.clone())));
    let write_end = Arc::new(UPSafeCell::new(Pipe::new(false, true, ring.clone())));
    ring.lock().set_write_point(Arc::downgrade(&write_end));
    ring.lock().set_read_point(Arc::downgrade(&read_end));
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
        pipe.read(buf)
    }

    fn write(&mut self, buf: &[u8]) -> Result<usize, VfsFsError> {
        let pipe = self.end.lock();
        pipe.write(buf)
    }
}
