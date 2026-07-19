#[repr(C)]
pub struct MailBoxdsc {
    pub op: u8,
    ///内核地址
    pub arg: u64,
    /// 序列号
    pub seq: u32,
    /// 完成标志
    pub done: bool,
}
