//! USB standard descriptor parsing and the persistent device-description tree.
//!
//! USB descriptors are byte streams, not naturally aligned Rust structures.
//! Decode every little-endian field explicitly instead of casting DMA memory to
//! `#[repr(C, packed)]` structures.

use alloc::{boxed::Box, vec::Vec};

const DESCRIPTOR_DEVICE: u8 = 1;
const DESCRIPTOR_CONFIGURATION: u8 = 2;
const DESCRIPTOR_INTERFACE: u8 = 4;
const DESCRIPTOR_ENDPOINT: u8 = 5;

const DEVICE_DESCRIPTOR_LENGTH: usize = 18;
const CONFIGURATION_DESCRIPTOR_LENGTH: usize = 9;
const INTERFACE_DESCRIPTOR_LENGTH: usize = 9;
const ENDPOINT_DESCRIPTOR_LENGTH: usize = 7;

/// A malformed standard descriptor stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DescriptorError {
    /// The supplied buffer ends before a required field.
    TooShort,
    /// A descriptor's type does not match the requested standard type.
    WrongType { expected: u8, found: u8 },
    /// A descriptor's `bLength` is shorter than its mandatory fields.
    InvalidLength { offset: usize, length: u8 },
    /// `wTotalLength` does not describe the received configuration body.
    InvalidTotalLength { total: usize, available: usize },
    /// An endpoint occurred before an interface/alternate-setting descriptor.
    EndpointWithoutInterface { offset: usize },
    /// One interface repeated the same alternate setting.
    DuplicateAlternateSetting { interface: u8, alternate: u8 },
    /// `bNumInterfaces` does not match the parsed interface numbers.
    InterfaceCountMismatch { expected: u8, found: usize },
    /// One alternate setting's `bNumEndpoints` does not match its endpoint list.
    EndpointCountMismatch {
        interface: u8,
        alternate: u8,
        expected: u8,
        found: usize,
    },
}

/// USB class, subclass and protocol values as advertised by a descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UsbClassCode {
    pub class: u8,
    pub subclass: u8,
    pub protocol: u8,
}

/// USB direction encoded in an endpoint address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndpointDirection {
    Out,
    In,
}

/// Endpoint transfer type encoded by `bmAttributes[1:0]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndpointTransferType {
    Control,
    Isochronous,
    Bulk,
    Interrupt,
}

/// Parsed `bEndpointAddress`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EndpointAddress {
    pub number: u8,
    pub direction: EndpointDirection,
}

impl EndpointAddress {
    fn from_raw(value: u8) -> Self {
        Self {
            number: value & 0x0f,
            direction: if value & 0x80 != 0 {
                EndpointDirection::In
            } else {
                EndpointDirection::Out
            },
        }
    }
}

/// Parsed standard USB Device Descriptor (USB 2.0 9.6.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeviceDescriptor {
    pub usb_version: u16,
    pub class: UsbClassCode,
    pub max_packet_size0: u8,
    pub vendor_id: u16,
    pub product_id: u16,
    pub device_version: u16,
    pub manufacturer_string: u8,
    pub product_string: u8,
    pub serial_number_string: u8,
    pub configuration_count: u8,
}

impl DeviceDescriptor {
    /// Decode the complete, fixed 18-byte Device Descriptor.
    pub fn parse(bytes: &[u8]) -> Result<Self, DescriptorError> {
        if bytes.len() < DEVICE_DESCRIPTOR_LENGTH {
            return Err(DescriptorError::TooShort);
        }
        if bytes[0] < DEVICE_DESCRIPTOR_LENGTH as u8 {
            return Err(DescriptorError::InvalidLength {
                offset: 0,
                length: bytes[0],
            });
        }
        if bytes[1] != DESCRIPTOR_DEVICE {
            return Err(DescriptorError::WrongType {
                expected: DESCRIPTOR_DEVICE,
                found: bytes[1],
            });
        }
        Ok(Self {
            usb_version: le16(bytes, 2),
            class: UsbClassCode {
                class: bytes[4],
                subclass: bytes[5],
                protocol: bytes[6],
            },
            max_packet_size0: bytes[7],
            vendor_id: le16(bytes, 8),
            product_id: le16(bytes, 10),
            device_version: le16(bytes, 12),
            manufacturer_string: bytes[14],
            product_string: bytes[15],
            serial_number_string: bytes[16],
            configuration_count: bytes[17],
        })
    }
}

/// Parsed standard Configuration Descriptor (USB 2.0 9.6.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConfigurationDescriptor {
    pub total_length: u16,
    pub interface_count: u8,
    pub value: u8,
    pub configuration_string: u8,
    pub attributes: u8,
    pub max_power_units: u8,
}

impl ConfigurationDescriptor {
    /// Decode the fixed 9-byte configuration descriptor header.
    pub fn parse(bytes: &[u8]) -> Result<Self, DescriptorError> {
        if bytes.len() < CONFIGURATION_DESCRIPTOR_LENGTH {
            return Err(DescriptorError::TooShort);
        }
        if bytes[0] < CONFIGURATION_DESCRIPTOR_LENGTH as u8 {
            return Err(DescriptorError::InvalidLength {
                offset: 0,
                length: bytes[0],
            });
        }
        if bytes[1] != DESCRIPTOR_CONFIGURATION {
            return Err(DescriptorError::WrongType {
                expected: DESCRIPTOR_CONFIGURATION,
                found: bytes[1],
            });
        }
        Ok(Self {
            total_length: le16(bytes, 2),
            interface_count: bytes[4],
            value: bytes[5],
            configuration_string: bytes[6],
            attributes: bytes[7],
            max_power_units: bytes[8],
        })
    }
}

/// Parsed Interface Descriptor; one descriptor represents one alternate setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InterfaceDescriptor {
    pub number: u8,
    pub alternate_setting: u8,
    pub endpoint_count: u8,
    pub class: UsbClassCode,
    pub interface_string: u8,
}

impl InterfaceDescriptor {
    fn parse(bytes: &[u8], offset: usize) -> Result<Self, DescriptorError> {
        require_descriptor(
            bytes,
            offset,
            DESCRIPTOR_INTERFACE,
            INTERFACE_DESCRIPTOR_LENGTH,
        )?;
        Ok(Self {
            number: bytes[offset + 2],
            alternate_setting: bytes[offset + 3],
            endpoint_count: bytes[offset + 4],
            class: UsbClassCode {
                class: bytes[offset + 5],
                subclass: bytes[offset + 6],
                protocol: bytes[offset + 7],
            },
            interface_string: bytes[offset + 8],
        })
    }
}

/// Parsed Endpoint Descriptor (USB 2.0 9.6.6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EndpointDescriptor {
    pub address: EndpointAddress,
    pub transfer_type: EndpointTransferType,
    /// Synchronization and usage bits from `bmAttributes[5:2]`.
    pub attributes: u8,
    /// Raw `wMaxPacketSize`; helpers may decode payload and transaction bits.
    pub max_packet_size: u16,
    pub interval: u8,
}

impl EndpointDescriptor {
    fn parse(bytes: &[u8], offset: usize) -> Result<Self, DescriptorError> {
        require_descriptor(
            bytes,
            offset,
            DESCRIPTOR_ENDPOINT,
            ENDPOINT_DESCRIPTOR_LENGTH,
        )?;
        let attributes = bytes[offset + 3];
        let transfer_type = match attributes & 0x03 {
            0 => EndpointTransferType::Control,
            1 => EndpointTransferType::Isochronous,
            2 => EndpointTransferType::Bulk,
            _ => EndpointTransferType::Interrupt,
        };
        Ok(Self {
            address: EndpointAddress::from_raw(bytes[offset + 2]),
            transfer_type,
            attributes: (attributes >> 2) & 0x0f,
            max_packet_size: le16(bytes, offset + 4),
            interval: bytes[offset + 6],
        })
    }

    /// Payload bytes in `wMaxPacketSize[10:0]`.
    pub fn max_packet_payload(self) -> u16 {
        self.max_packet_size & 0x07ff
    }

    /// Number of transactions permitted in one USB high-speed microframe.
    /// For bulk and control endpoints this is always one.
    pub fn max_packet_transactions(self) -> u8 {
        if matches!(
            self.transfer_type,
            EndpointTransferType::Isochronous | EndpointTransferType::Interrupt
        ) {
            ((self.max_packet_size >> 11) & 0x03) as u8 + 1
        } else {
            1
        }
    }
}

/// A range in the configuration's immutable raw descriptor stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DescriptorRange {
    offset: u16,
    length: u16,
}

impl DescriptorRange {
    fn new(offset: usize, length: usize) -> Self {
        Self {
            offset: offset as u16,
            length: length as u16,
        }
    }

    /// Borrow this opaque descriptor from its parent configuration.
    pub fn bytes<'a>(self, configuration: &'a UsbConfiguration) -> &'a [u8] {
        let start = usize::from(self.offset);
        &configuration.raw[start..start + usize::from(self.length)]
    }
}

/// One alternate setting and the endpoints active when it is selected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsbAlternateSetting {
    pub descriptor: InterfaceDescriptor,
    pub endpoints: Vec<EndpointDescriptor>,
    pub class_specific: Vec<DescriptorRange>,
}

/// All alternate settings of one USB interface number.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsbInterface {
    pub number: u8,
    pub alternate_settings: Vec<UsbAlternateSetting>,
}

impl UsbInterface {
    /// Return the alternate setting selected by its USB descriptor value.
    pub fn alternate_setting(&self, alternate: u8) -> Option<&UsbAlternateSetting> {
        self.alternate_settings
            .iter()
            .find(|setting| setting.descriptor.alternate_setting == alternate)
    }
}

/// A parsed configuration plus exactly one persistent copy of its raw stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsbConfiguration {
    pub descriptor: ConfigurationDescriptor,
    raw: Box<[u8]>,
    pub interfaces: Vec<UsbInterface>,
    pub class_specific: Vec<DescriptorRange>,
}

impl UsbConfiguration {
    /// Decode and retain a complete configuration descriptor stream.
    pub fn parse(bytes: &[u8]) -> Result<Self, DescriptorError> {
        let descriptor = ConfigurationDescriptor::parse(bytes)?;
        let total = usize::from(descriptor.total_length);
        if total < CONFIGURATION_DESCRIPTOR_LENGTH || total > bytes.len() {
            return Err(DescriptorError::InvalidTotalLength {
                total,
                available: bytes.len(),
            });
        }

        let raw = Box::<[u8]>::from(&bytes[..total]);
        let mut configuration = Self {
            descriptor,
            raw,
            interfaces: Vec::new(),
            class_specific: Vec::new(),
        };
        let mut current: Option<(usize, usize)> = None;
        let mut offset = CONFIGURATION_DESCRIPTOR_LENGTH;
        while offset < total {
            if offset + 2 > total {
                return Err(DescriptorError::TooShort);
            }
            let length = usize::from(configuration.raw[offset]);
            if length < 2 || offset + length > total {
                return Err(DescriptorError::InvalidLength {
                    offset,
                    length: configuration.raw[offset],
                });
            }
            match configuration.raw[offset + 1] {
                DESCRIPTOR_INTERFACE => {
                    let parsed = InterfaceDescriptor::parse(&configuration.raw, offset)?;
                    let interface_index = configuration
                        .interfaces
                        .iter()
                        .position(|interface| interface.number == parsed.number)
                        .unwrap_or_else(|| {
                            configuration.interfaces.push(UsbInterface {
                                number: parsed.number,
                                alternate_settings: Vec::new(),
                            });
                            configuration.interfaces.len() - 1
                        });
                    let interface = &mut configuration.interfaces[interface_index];
                    if interface.alternate_settings.iter().any(|alternate| {
                        alternate.descriptor.alternate_setting == parsed.alternate_setting
                    }) {
                        return Err(DescriptorError::DuplicateAlternateSetting {
                            interface: parsed.number,
                            alternate: parsed.alternate_setting,
                        });
                    }
                    interface.alternate_settings.push(UsbAlternateSetting {
                        descriptor: parsed,
                        endpoints: Vec::new(),
                        class_specific: Vec::new(),
                    });
                    current = Some((interface_index, interface.alternate_settings.len() - 1));
                }
                DESCRIPTOR_ENDPOINT => {
                    let (interface_index, alternate_index) =
                        current.ok_or(DescriptorError::EndpointWithoutInterface { offset })?;
                    let endpoint = EndpointDescriptor::parse(&configuration.raw, offset)?;
                    configuration.interfaces[interface_index].alternate_settings[alternate_index]
                        .endpoints
                        .push(endpoint);
                }
                _ => {
                    let range = DescriptorRange::new(offset, length);
                    if let Some((interface_index, alternate_index)) = current {
                        configuration.interfaces[interface_index].alternate_settings
                            [alternate_index]
                            .class_specific
                            .push(range);
                    } else {
                        configuration.class_specific.push(range);
                    }
                }
            }
            offset += length;
        }
        if configuration.interfaces.len() != usize::from(configuration.descriptor.interface_count) {
            return Err(DescriptorError::InterfaceCountMismatch {
                expected: configuration.descriptor.interface_count,
                found: configuration.interfaces.len(),
            });
        }
        for interface in &configuration.interfaces {
            for setting in &interface.alternate_settings {
                if setting.endpoints.len() != usize::from(setting.descriptor.endpoint_count) {
                    return Err(DescriptorError::EndpointCountMismatch {
                        interface: setting.descriptor.number,
                        alternate: setting.descriptor.alternate_setting,
                        expected: setting.descriptor.endpoint_count,
                        found: setting.endpoints.len(),
                    });
                }
            }
        }
        Ok(configuration)
    }

    /// Return the raw, exact `wTotalLength` descriptor stream.
    pub fn raw(&self) -> &[u8] {
        &self.raw
    }

    /// Find one interface's requested alternate setting.
    pub fn alternate_setting(
        &self,
        interface_number: u8,
        alternate: u8,
    ) -> Option<&UsbAlternateSetting> {
        self.interfaces
            .iter()
            .find(|interface| interface.number == interface_number)?
            .alternate_settings
            .iter()
            .find(|setting| setting.descriptor.alternate_setting == alternate)
    }
}

fn require_descriptor(
    bytes: &[u8],
    offset: usize,
    expected_type: u8,
    minimum_length: usize,
) -> Result<(), DescriptorError> {
    if offset + minimum_length > bytes.len() {
        return Err(DescriptorError::TooShort);
    }
    if usize::from(bytes[offset]) < minimum_length {
        return Err(DescriptorError::InvalidLength {
            offset,
            length: bytes[offset],
        });
    }
    if bytes[offset + 1] != expected_type {
        return Err(DescriptorError::WrongType {
            expected: expected_type,
            found: bytes[offset + 1],
        });
    }
    Ok(())
}

fn le16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

#[cfg(test)]
mod tests {
    use super::{DeviceDescriptor, EndpointDirection, UsbConfiguration};

    #[test]
    fn parses_device_descriptor() {
        let bytes = [
            18, 1, 0x00, 0x02, 0, 0, 0, 64, 0x34, 0x12, 0x78, 0x56, 0x00, 0x01, 1, 2, 3, 1,
        ];
        let descriptor = DeviceDescriptor::parse(&bytes).unwrap();
        assert_eq!(descriptor.vendor_id, 0x1234);
        assert_eq!(descriptor.product_id, 0x5678);
        assert_eq!(descriptor.max_packet_size0, 64);
    }

    #[test]
    fn groups_interfaces_alternates_and_endpoints() {
        let bytes = [
            9, 2, 41, 0, 1, 1, 0, 0x80, 50, 9, 4, 0, 0, 1, 3, 1, 1, 0, 7, 5, 0x81, 3, 8, 0, 10, 9,
            4, 0, 1, 1, 3, 1, 2, 0, 7, 5, 0x02, 2, 64, 0, 0,
        ];
        let configuration = UsbConfiguration::parse(&bytes).unwrap();
        assert_eq!(configuration.interfaces.len(), 1);
        let interface = &configuration.interfaces[0];
        assert_eq!(interface.alternate_settings.len(), 2);
        assert_eq!(
            interface.alternate_settings[0].endpoints[0]
                .address
                .direction,
            EndpointDirection::In
        );
        assert!(configuration.alternate_setting(0, 1).is_some());
    }
}
