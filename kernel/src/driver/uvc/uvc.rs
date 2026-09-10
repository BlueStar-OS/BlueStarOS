//! Minimal USB Video Class (UVC) driver.
//!
//! This first step only claims UVC interfaces and retains their xHCI device
//! handle. Streaming endpoints and video controls belong in the next layer.

use crate::driver::usb::xhci_host::descriptor::UsbInterface;
use crate::driver::usb::xhci_host::device::XhciDeviceRef;
use crate::driver::usb::xhci_host::driver::{
    UsbDeviceDriver, UsbDeviceDriverFactory, UsbDeviceMatcher, UsbInterfaceMatcher,
};
use crate::sync::NoIrqLock;
use alloc::sync::Arc;
use log::info;

const USB_CLASS_VIDEO: u8 = 0x0e;

/// One UVC interface accepted by the generic video driver.
pub struct UvcVideo {
    pub device: XhciDeviceRef,
    pub interface: Arc<UsbInterface>,
}

/// One runtime UVC driver instance. It owns exactly the interface it claimed.
pub struct UvcDriver {
    video: NoIrqLock<Option<UvcVideo>>,
}

impl UvcDriver {
    fn new() -> Self {
        Self {
            video: NoIrqLock::new(None),
        }
    }
}

impl UsbDeviceDriver for UvcDriver {
    fn probe(
        &self,
        device: XhciDeviceRef,
        interface: Arc<UsbInterface>,
    ) -> Result<(), &'static str> {
        let (port_id, slot_id, device_address) = device.lock(|device| {
            (
                device.port_id,
                device.slot_id.unwrap_or(0),
                device.device_address.unwrap_or(0),
            )
        });
        info!(
            "uvc: probe video device: port={}, slot={}, address={}, interface={}",
            port_id, slot_id, device_address, interface.number
        );
        let interface_number = interface.number;
        self.video.lock(|video| {
            if video.is_some() {
                return Err("UVC driver instance already owns an interface");
            }
            *video = Some(UvcVideo { device, interface });
            Ok(())
        })?;
        info!("uvc: video interface {} claimed", interface_number);
        Ok(())
    }
}

/// Shared factory used by the global USB match table.
pub struct UvcDriverFactory;

impl UsbDeviceDriverFactory for UvcDriverFactory {
    fn new_driver(&self) -> Arc<dyn UsbDeviceDriver> {
        Arc::new(UvcDriver::new())
    }
}

/// Register the generic UVC class rule.
pub fn register() -> UsbDeviceMatcher {
    let driver: Arc<dyn UsbDeviceDriverFactory> = Arc::new(UvcDriverFactory);
    UsbDeviceMatcher {
        vendor_id: None,
        product_id: None,
        interface: UsbInterfaceMatcher {
            class: Some(USB_CLASS_VIDEO),
            subclass: None,
            protocol: None,
        },
        driver,
    }
}
