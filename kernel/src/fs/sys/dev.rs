//! USB records exported through `/sys/bus/usb/devices/list`.
//!
//! A record is intentionally a small text-oriented snapshot, not a device
//! object.  Drivers retain ownership of their real hardware state; this layer
//! merely lets user space discover devices that completed descriptor parsing.

use crate::fs::vfs::{File, VfsFsError, VfsStat, VFS_DT_REG};
use crate::sync::NoIrqLock;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::any::Any;
use core::fmt::Write;
use lazy_static::lazy_static;
use spin::Mutex;

/// One USB device that completed xHCI descriptor enumeration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UsbDeviceRecord {
    /// One-based host-controller bus number.
    pub bus_number: u8,
    /// xHCI-assigned USB address, shown as Linux's `Device` number.
    pub device_number: u8,
    /// One-based root-port number; used only to replace/remove records.
    pub port_number: u8,
    pub vendor_id: u16,
    pub product_id: u16,
    /// Number of configuration descriptor trees parsed for this device.
    pub configuration_count: u8,
}

lazy_static! {
    static ref USB_DEVICES: NoIrqLock<Vec<UsbDeviceRecord>> = NoIrqLock::new(Vec::new());
}

/// Insert or replace the snapshot for one bus/port pair.
pub fn record_usb_device(record: UsbDeviceRecord) {
    USB_DEVICES.lock(|devices| {
        if let Some(existing) = devices.iter_mut().find(|existing| {
            existing.bus_number == record.bus_number && existing.port_number == record.port_number
        }) {
            *existing = record;
        } else {
            devices.push(record);
        }
    });
}

/// Remove a snapshot when its root port disconnects.
pub fn remove_usb_device(bus_number: u8, port_number: u8) {
    USB_DEVICES.lock(|devices| {
        devices
            .retain(|device| device.bus_number != bus_number || device.port_number != port_number);
    });
}

fn render_usb_devices() -> String {
    USB_DEVICES.lock(|devices| {
        let mut output = String::new();
        for device in devices.iter() {
            // Fixed fields keep the first four columns trivial for the tiny
            // no_std lsusb parser: bus, address, VID, PID.
            let _ = writeln!(
                output,
                "{:03} {:03} {:04x} {:04x} {}",
                device.bus_number,
                device.device_number,
                device.vendor_id,
                device.product_id,
                device.configuration_count,
            );
        }
        output
    })
}

struct UsbDevicesFile {
    offset: Mutex<usize>,
}

impl File for UsbDevicesFile {
    fn read(&self, buffer: &mut [u8]) -> Result<usize, VfsFsError> {
        let output = render_usb_devices();
        let mut offset = self.offset.lock();
        if *offset >= output.len() {
            // RamFs device nodes share their File object across opens. Reset
            // after EOF so the next minimal reader starts at byte zero.
            *offset = 0;
            return Ok(0);
        }
        let bytes = output.as_bytes();
        let length = core::cmp::min(buffer.len(), bytes.len() - *offset);
        buffer[..length].copy_from_slice(&bytes[*offset..*offset + length]);
        *offset += length;
        Ok(length)
    }

    fn write(&self, _buffer: &[u8]) -> Result<usize, VfsFsError> {
        Err(VfsFsError::PermissionDenied)
    }

    fn read_at(&self, offset: usize, buffer: &mut [u8]) -> Result<usize, VfsFsError> {
        let output = render_usb_devices();
        if offset >= output.len() {
            return Ok(0);
        }
        let bytes = output.as_bytes();
        let length = core::cmp::min(buffer.len(), bytes.len() - offset);
        buffer[..length].copy_from_slice(&bytes[offset..offset + length]);
        Ok(length)
    }

    fn lseek(&self, offset: isize, whence: usize) -> Result<usize, VfsFsError> {
        let output_length = render_usb_devices().len() as isize;
        let mut cursor = self.offset.lock();
        let next = match whence {
            0 => offset,
            1 => *cursor as isize + offset,
            2 => output_length + offset,
            _ => return Err(VfsFsError::Invalid),
        };
        if next < 0 {
            return Err(VfsFsError::Invalid);
        }
        *cursor = next as usize;
        Ok(*cursor)
    }

    fn stat(&self) -> Result<VfsStat, VfsFsError> {
        Ok(VfsStat {
            inode: 0,
            size: render_usb_devices().len() as u64,
            mode: 0,
            file_type: VFS_DT_REG,
        })
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Create the dynamic text node mounted at `/sys/bus/usb/devices/list`.
pub fn usb_devices_file() -> Arc<dyn File> {
    Arc::new(UsbDevicesFile {
        offset: Mutex::new(0),
    })
}

#[cfg(test)]
mod tests {
    use super::{
        record_usb_device, remove_usb_device, render_usb_devices, UsbDeviceRecord, USB_DEVICES,
    };

    #[test]
    fn records_are_replaced_by_bus_and_port() {
        USB_DEVICES.lock(|devices| devices.clear());
        record_usb_device(UsbDeviceRecord {
            bus_number: 1,
            device_number: 2,
            port_number: 3,
            vendor_id: 0x1234,
            product_id: 0x5678,
            configuration_count: 1,
        });
        record_usb_device(UsbDeviceRecord {
            bus_number: 1,
            device_number: 4,
            port_number: 3,
            vendor_id: 0xabcd,
            product_id: 0xef01,
            configuration_count: 2,
        });
        assert_eq!(render_usb_devices(), "001 004 abcd ef01 2\n");
        remove_usb_device(1, 3);
        assert!(render_usb_devices().is_empty());
    }
}
