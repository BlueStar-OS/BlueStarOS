//! 网络缓冲区 — 类似 Linux sk_buff 的四指针模型
//!
//! 内存布局:
//! ```text
//! head          data          tail          end
//! |             |             |             |
//! v             v             v             v
//! [---headroom---[===数据部分===]---tailroom---]
//! ```
//!
//! 过程流: 数据收/发时四指针配合移动
//!
//! 收包 (网卡DMA写入 → 协议栈):
//!   data ──pull──→ (剥离以太网头,指向IP头)   → eth_type_trans
//!   data ──pull──→ (剥离IP头,指向TCP头)      → tcp_v4_rcv
//!
//! 发包 (协议栈 → 网卡DMA发送):
//!   data ──push──→ (前移data,填入以太网头)   → 网卡从data开始发
//!   tail ──put──→  (后移tail,添加payload)    → skb_put_data
//!
//! 参考 Linux: include/linux/skbuff.h

use core::mem;

/// 缓冲区总大小 (2KB, 含数据和全部协议头开销)
const NETBUFFERSIZE: usize = 1 << 12;

/// 默认预留的协议头空间 (二层+三层+四层最大头)
///
/// 参考 Linux: 驱动中 skb_reserve(skb, NET_IP_ALIGN) + MAX_HEADER
const DEFAULT_HEADROOM: usize = 256;

/// 网络缓冲区
///
/// 对应 Linux: struct sk_buff (极简版, 仅线性数据区)
/// 参考 Linux: include/linux/skbuff.h 四指针模型:
///   - head:  skb->head   — 分配区起始
///   - data:  skb->data   — 当前协议层数据起始
///   - tail:  skb->tail   — 数据结束
///   - end:   skb->end    — 分配区结束
pub struct NetBuffer {
    buffer: [u8; NETBUFFERSIZE],
    pub head: usize,
    pub data: usize,
    pub tail: usize,
    pub end: usize,
}

impl NetBuffer {
    /// 推进data指针
    pub fn push_data_ptr(&mut self, len: usize) -> bool {
        if self.data + len >= self.end {
            return false;
        }
        self.data += len;
        true
    }

    /// 分配新缓冲区, 预留默认头部空间
    ///
    /// 对应 Linux 流程:
    ///   alloc_skb(size)              → __build_skb → head=data=tail=0, end=size
    ///   skb_reserve(skb, headroom)   → data+=len, tail+=len
    ///
    /// Linux 参考: net/core/skbuff.c 第306行 __build_skb
    ///           include/linux/skbuff.h 第2340行 skb_reserve
    pub fn new() -> Self {
        let mut buf = NetBuffer {
            buffer: [0u8; NETBUFFERSIZE],
            head: 0,
            data: 0,
            tail: 0,
            end: NETBUFFERSIZE,
        };
        // 预留头部空间: 相当于 skb_reserve(skb, DEFAULT_HEADROOM)
        buf.data = DEFAULT_HEADROOM;
        buf.tail = DEFAULT_HEADROOM;
        buf
    }

    /// 用现有数据填充缓冲区, 预留头部空间
    ///
    /// 调用前清除原内容, 整个过程相当于:
    ///   skb_reserve(skb, headroom) + skb_put_data(skb, data, len)
    ///
    /// 参考 Linux: include/linux/skbuff.h 第2232行 skb_put_data
    pub fn new_data(&mut self, data: &[u8]) {
        // 重置四指针到初始状态
        self.head = 0;
        self.data = 0;
        self.tail = 0;
        self.end = NETBUFFERSIZE;

        // step 1: skb_reserve — 预留协议头空间
        self.data = DEFAULT_HEADROOM;
        self.tail = DEFAULT_HEADROOM;

        // step 2: skb_put_data — 将数据拷贝到尾部
        let dst = self.put(data.len());
        dst.copy_from_slice(data);
    }

    /// 在前面添加协议头 (对应 Linux: skb_push)
    ///
    /// 泛型版本支持以太网头 / IPv4 头 / UDP 头等任意 packed 协议头。
    /// T 必须为 `#[repr(C, packed)]` 类型，保证 `size_of::<T>()` 即线缆长度。
    ///
    /// 过程: data 指针前移 `sizeof(T)`, 拷贝协议头内存到新 data 位置。
    ///
    /// 参考 Linux: include/linux/skbuff.h 第2248行 __skb_push:
    ///   skb->data -= len;
    ///   skb->len  += len;
    ///   return skb->data;
    pub fn new_agreement_head<T>(&mut self, agreement: &T) {
        let head_size = mem::size_of::<T>();
        assert!(
            self.data >= head_size,
            "new_agreement_head: not enough headroom"
        );
        self.data -= head_size;

        // SAFETY: T 必须是 #[repr(C, packed)] 类型（由调用者保证），
        // 无填充，可直接按字节拷贝到缓冲区头部。
        unsafe {
            let src = agreement as *const T as *const u8;
            let dst = &mut self.buffer[self.data] as *mut u8;
            core::ptr::copy_nonoverlapping(src, dst, head_size);
        }
    }

    // ==================== skb 四指针模型核心操作 ====================

    /// 在尾部追加 len 字节空间, 返回可变切片 (对应 Linux: skb_put)
    ///
    /// 过程: tail 指针后移, 返回旧 tail 位置的可变引用
    ///
    /// 参考 Linux: include/linux/skbuff.h 第2192行 __skb_put:
    ///   skb->tail += len;
    ///   skb->len  += len;
    ///   return old_tail;
    pub fn put(&mut self, len: usize) -> &mut [u8] {
        let old_tail = self.tail;
        self.tail += len;
        assert!(self.tail <= self.end, "put 超过缓冲区尾部");
        &mut self.buffer[old_tail..self.tail]
    }

    /// 拷贝数据到尾部 (对应 Linux: skb_put_data)
    ///
    /// 参考 Linux: include/linux/skbuff.h 第2232行:
    ///   void *skb_put_data(struct sk_buff *skb, const void *data, unsigned int len)
    pub fn put_data(&mut self, data: &[u8]) {
        let dst = self.put(data.len());
        dst.copy_from_slice(data);
    }

    /// 在前面预留 len 字节空间, 返回可变切片 (对应 Linux: skb_push)
    ///
    /// 过程: data 指针前移, 返回新 data 位置的可变引用
    ///
    /// 参考 Linux: include/linux/skbuff.h 第2248行:
    ///   skb->data -= len;
    ///   skb->len  += len;
    ///   return skb->data;
    pub fn push(&mut self, len: usize) -> &mut [u8] {
        assert!(self.data >= len, "push 超过 headroom");
        self.data -= len;
        &mut self.buffer[self.data..self.data + len]
    }

    /// 从前面移除 len 字节 (对应 Linux: skb_pull)
    ///
    /// 过程: data 指针后移, 相当于剥离当前协议头
    ///
    /// 参考 Linux: include/linux/skbuff.h 第2256行:
    ///   skb->data += len;
    ///   skb->len  -= len;
    pub fn pull(&mut self, len: usize) {
        assert!(self.data + len <= self.tail, "pull 超过数据区");
        self.data += len;
    }

    /// 预留头部空间 (对应 Linux: skb_reserve)
    ///
    /// 只能在空缓冲区调用(data == tail == 0), data 和 tail 同时前移
    ///
    /// 参考 Linux: include/linux/skbuff.h 第2340行:
    ///   skb->data += len;
    ///   skb->tail += len;
    pub fn reserve(&mut self, len: usize) {
        assert!(self.data == self.tail, "skb_reserve 必须在空缓冲区调用");
        self.data = len;
        self.tail = len;
    }

    /// 清空数据区, 保留已预留的 headroom
    pub fn clear(&mut self) {
        self.data = DEFAULT_HEADROOM;
        self.tail = DEFAULT_HEADROOM;
    }

    // ==================== 查询方法 ====================

    /// 数据长度 tail - data (对应 Linux: skb->len)
    pub fn data_len(&self) -> usize {
        self.tail - self.data
    }

    /// 头部空闲空间 data - head (对应 Linux: skb_headroom)
    pub fn headroom(&self) -> usize {
        self.data - self.head
    }

    /// 尾部空闲空间 end - tail (对应 Linux: skb_tailroom)
    pub fn tailroom(&self) -> usize {
        self.end - self.tail
    }

    /// 返回数据切片
    pub fn data_slice(&self) -> &[u8] {
        &self.buffer[self.data..self.tail]
    }

    /// 返回可变数据切片
    pub fn data_slice_mut(&mut self) -> &mut [u8] {
        &mut self.buffer[self.data..self.tail]
    }

    /// 返回 data 指针 (给DMA/硬件用)
    pub fn data_ptr(&self) -> *const u8 {
        &self.buffer[self.data] as *const u8
    }

    /// 返回 data 可变指针 (给DMA/硬件用)
    pub fn data_mut_ptr(&mut self) -> *mut u8 {
        &mut self.buffer[self.data] as *mut u8
    }

    /// 是否包含有效数据
    pub fn is_empty(&self) -> bool {
        self.tail == self.data
    }
}
