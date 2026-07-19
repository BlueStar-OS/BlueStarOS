//! e1000 设备实例与全局驱动状态。

use core::sync::atomic::{AtomicBool, AtomicUsize};

use lazy_static::lazy_static;

use crate::driver::network::e1000::queue::{rx::E1000RxRing, tx::E1000TxRing};
use crate::driver::pcie::BarSpace;
use crate::sync::UPSafeCell;

/// e1000 网卡设备实例。
///
/// 持有 BAR MMIO、收发描述符环和站点 MAC 地址。
pub struct E1000 {
    /// BAR0 MMIO 空间。
    pub bar: BarSpace,
    /// 接收描述符环。
    pub rx_ring: E1000RxRing,
    /// 发送描述符环。
    pub tx_ring: E1000TxRing,
    /// 站点 MAC 地址。
    pub mac: [u8; 6],
}

lazy_static! {
    /// 全局 e1000 设备实例。
    pub static ref E1000_DEV: UPSafeCell<Option<E1000>> =
        UPSafeCell::new(None);
}

/// BAR0 MMIO 基址快照，供中断上下文无锁访问。
pub(crate) static E1000_MMIO_BASE: AtomicUsize = AtomicUsize::new(0);

/// RX 中断待处理标志。
pub static E1000_RX_INTR_PENDING: AtomicBool = AtomicBool::new(false);
