//! Small USB interface-driver registry.
//!
//! xHCI owns the USB device. This module only decides which class driver
//! claims each parsed interface and gives that driver a shared device handle.

use super::descriptor::{DeviceDescriptor, UsbInterface};
use super::device::XhciDeviceRef;
use crate::sync::NoIrqLock;
use alloc::sync::Arc;
use alloc::vec::Vec;
use lazy_static::lazy_static;

/// A USB driver instance receives the xHCI device and the interface it
/// claimed. One instance belongs to one claimed interface.
pub trait UsbDeviceDriver: Send + Sync {
    fn probe(
        &self,
        device: XhciDeviceRef,
        interface: Arc<UsbInterface>,
    ) -> Result<(), &'static str>;
}

/// Factory kept by the shared match table.
///
/// The factory is shared; `new_driver` creates the stateful driver instance
/// only after one concrete USB interface matches.
pub trait UsbDeviceDriverFactory: Send + Sync {
    fn new_driver(&self) -> Arc<dyn UsbDeviceDriver>;
}

/// The part of an interface descriptor used by a class-driver match.
/// `None` is a wildcard.
#[derive(Clone, Copy, Debug, Default)]
pub struct UsbInterfaceMatcher {
    pub class: Option<u8>,
    pub subclass: Option<u8>,
    pub protocol: Option<u8>,
}

impl UsbInterfaceMatcher {
    fn matches(self, interface: &UsbInterface) -> bool {
        interface.alternate_settings.iter().any(|alternate| {
            let class = alternate.descriptor.class;
            self.class.map_or(true, |value| value == class.class)
                && self.subclass.map_or(true, |value| value == class.subclass)
                && self.protocol.map_or(true, |value| value == class.protocol)
        })
    }
}

/// One shared driver-table entry. VID/PID `None` means any device.
#[derive(Clone)]
pub struct UsbDeviceMatcher {
    pub vendor_id: Option<u16>,
    pub product_id: Option<u16>,
    pub interface: UsbInterfaceMatcher,
    pub driver: Arc<dyn UsbDeviceDriverFactory>,
}

impl UsbDeviceMatcher {
    pub(crate) fn matches(&self, descriptor: DeviceDescriptor, interface: &UsbInterface) -> bool {
        self.vendor_id
            .map_or(true, |value| value == descriptor.vendor_id)
            && self
                .product_id
                .map_or(true, |value| value == descriptor.product_id)
            && self.interface.matches(interface)
    }
}

/// One driver table shared by all xHCI hosts.
#[derive(Default)]
pub struct UsbDeviceDriverTable {
    pub(crate) entries: Vec<UsbDeviceMatcher>,
}

lazy_static! {
    /// The one USB driver table used by every xHCI host.
    pub static ref USBDRIERTABLE: NoIrqLock<Option<UsbDeviceDriverTable>> =
        NoIrqLock::new(None);
}

/// Register one class-driver match rule in the shared table.
pub fn register_usb_device_driver(matcher: UsbDeviceMatcher) {
    USBDRIERTABLE.lock(|table| {
        table
            .get_or_insert_with(UsbDeviceDriverTable::default)
            .entries
            .push(matcher);
    });
}

/// Register all built-in USB drivers in one central place.
pub fn register_all_usbdriver() {
    register_usb_device_driver(crate::driver::uvc::register());
}

/// Create the shared table and populate it exactly once.
pub(crate) fn ensure_table_exist() {
    let create_table = USBDRIERTABLE.lock(|table| {
        if table.is_some() {
            return false;
        }
        *table = Some(UsbDeviceDriverTable::default());
        true
    });
    if create_table {
        register_all_usbdriver();
    }
}

/// One claimed interface and its driver, retained by an xHCI device.
#[derive(Clone)]
pub struct DeviceidTableEntry {
    pub interface: Arc<UsbInterface>,
    pub driver: Arc<dyn UsbDeviceDriver>,
}

/// Drivers claimed by one xHCI device.
#[derive(Default)]
pub struct DeviceidTable {
    entries: Vec<DeviceidTableEntry>,
}

impl DeviceidTable {
    pub fn register(
        &mut self,
        interface: Arc<UsbInterface>,
        driver: Arc<dyn UsbDeviceDriver>,
    ) -> Result<(), &'static str> {
        if self
            .entries
            .iter()
            .any(|entry| entry.interface.number == interface.number)
        {
            return Err("USB interface is already claimed");
        }
        self.entries.push(DeviceidTableEntry { interface, driver });
        Ok(())
    }
}
