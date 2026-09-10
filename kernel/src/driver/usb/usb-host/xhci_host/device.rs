//! Common xHCI root-port reset and device bookkeeping.

use super::descriptor::{
    DeviceDescriptor, EndpointDescriptor, EndpointDirection, EndpointTransferType,
    UsbAlternateSetting, UsbConfiguration,
};
use super::driver::{ensure_table_exist, DeviceidTable, USBDRIERTABLE};
use super::event::{
    portsc_neutral_value, CommandCompletionEvent, CompletionCode, PortLinkState, PortSpeed,
    PortStatusChangeEvent, TransferEvent, UsbDataStage, UsbSetup, XhciCommand,
};
use super::{
    Trb, XhciHost, DMA_PAGE_SIZE, PORT_REGISTER_BASE, PORT_REGISTER_STRIDE, TRBS_PER_RING,
    TRB_CYCLE, TRB_LINK_TOGGLE_CYCLE, TRB_SIZE, TRB_TYPE_LINK,
};
use crate::arch::riscv64::driver::dma::DmaMemory;
use crate::config::CPU_CIRCLE;
use crate::fs::sys::dev::UsbDeviceRecord;
use crate::sync::NoIrqLock;
use crate::time::get_time_tick;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ptr::{read_volatile, write_volatile};
use log::{error, info};

const PORTSC_CCS: u32 = 1 << 0;
const PORTSC_PP: u32 = 1 << 9;
const PORTSC_PR: u32 = 1 << 4;
const PORTSC_WPR: u32 = 1 << 31;
const PORTSC_PRC: u32 = 1 << 21;
const PORTSC_WRC: u32 = 1 << 19;
const PORTSC_CSC: u32 = 1 << 17;

// Input Control Context, xHCI 6.2.5.1.
const INPUT_CONTROL_CONTEXT_DROP_CONTEXT_OFFSET: usize = 0x00;
const INPUT_CONTROL_CONTEXT_ADD_CONTEXT_OFFSET: usize = 0x04;
const INPUT_CONTROL_CONTEXT_ADD_SLOT: u32 = 1 << 0;
const INPUT_CONTROL_CONTEXT_ADD_EP0: u32 = 1 << 1;

// Slot Context, xHCI 6.2.2.1.
const SLOT_CONTEXT_DWORD0_OFFSET: usize = 0x00;
const SLOT_CONTEXT_DWORD1_OFFSET: usize = 0x04;
const SLOT_CONTEXT_SPEED_SHIFT: u32 = 20;
const SLOT_CONTEXT_CONTEXT_ENTRIES_SHIFT: u32 = 27;
const SLOT_CONTEXT_INITIAL_CONTEXT_ENTRIES: u32 = 1 << SLOT_CONTEXT_CONTEXT_ENTRIES_SHIFT;
const SLOT_CONTEXT_ROOT_PORT_SHIFT: u32 = 16;

// Endpoint Context, xHCI 6.2.3.1. Endpoint 0 is context index 1.
const ENDPOINT_CONTEXT_DWORD0_OFFSET: usize = 0x00;
const ENDPOINT_CONTEXT_DWORD1_OFFSET: usize = 0x04;
const ENDPOINT_CONTEXT_TR_DEQUEUE_LO_OFFSET: usize = 0x08;
const ENDPOINT_CONTEXT_TR_DEQUEUE_HI_OFFSET: usize = 0x0c;
const ENDPOINT_CONTEXT_DWORD4_OFFSET: usize = 0x10;
// DWORD 0: EP State [2:0], Mult [9:8], MaxPStreams [14:10], LSA [15],
// Interval [23:16], Max ESIT Payload High [31:24].
const ENDPOINT_CONTEXT_CERR: u32 = 3 << 1;
const ENDPOINT_CONTEXT_EP_TYPE_CONTROL: u32 = 4 << 3;
const ENDPOINT_CONTEXT_MULT_SHIFT: u32 = 8;
const ENDPOINT_CONTEXT_INTERVAL_SHIFT: u32 = 16;
const ENDPOINT_CONTEXT_MAX_ESIT_PAYLOAD_HIGH_SHIFT: u32 = 24;
// DWORD 1: CErr [2:1], EP Type [5:3], HID [7], Max Burst [15:8],
// Max Packet Size [31:16].
const ENDPOINT_CONTEXT_EP_TYPE_SHIFT: u32 = 3;
const ENDPOINT_CONTEXT_MAX_BURST_SHIFT: u32 = 8;
const ENDPOINT_CONTEXT_MAX_PACKET_SIZE_SHIFT: u32 = 16;
const ENDPOINT_CONTEXT_MAX_PACKET_SIZE_MASK: u32 = 0xffff << ENDPOINT_CONTEXT_MAX_PACKET_SIZE_SHIFT;
const ENDPOINT_CONTEXT_MAX_ESIT_PAYLOAD_LOW_SHIFT: u32 = 16;
const ENDPOINT_CONTEXT_DCS: u32 = 1 << 0;
const ENDPOINT_CONTEXT_AVERAGE_TRB_LENGTH: u32 = 8;

// xHCI doorbell array, xHCI 5.6.1.
const MAX_ENDPOINT_NUMBER: u8 = 15;

// USB standard Get Descriptor request, USB 2.0 Tables 9-4 and 9-5.
const USB_REQUEST_GET_DESCRIPTOR: u8 = 6;
const USB_REQUEST_SET_CONFIGURATION: u8 = 9;
const USB_DESCRIPTOR_DEVICE: u16 = 1;
const USB_DESCRIPTOR_CONFIGURATION: u16 = 2;
const USB_DEVICE_DESCRIPTOR_LENGTH: usize = 18;
const EP0_DCI: u8 = 1;

/// Stable host-local identity used before Enable Slot assigns an xHCI slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DeviceId(pub(crate) u32);

/// Coarse lifecycle state. Request data lives in `pending`, not in this enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum XhciDeviceState {
    SlotEnabling,
    Addressing,
    Enumerating,
    UpdatingEndpoint0,
    ConfiguringEndpoints,
    SettingConfiguration,
    Configured,
    Disconnecting,
    Dead,
}

/// One operation currently owned by a device.
enum PendingOperation {
    EnableSlot,
    AddressDevice,
    EvaluateContext,
    Control {
        transfer: PendingControlTransfer,
        status_trb_pointer: u64,
    },
    ConfigureEndpoint {
        configuration_value: u8,
    },
    DisableSlot,
}

/// Semantic purpose and DMA lifetime for one EP0 control transfer.
enum PendingControlTransfer {
    DeviceDescriptor {
        buffer: DmaMemory,
    },
    ConfigurationHeader {
        index: u8,
        buffer: DmaMemory,
    },
    ConfigurationBody {
        index: u8,
        /// Exact wTotalLength from the configuration header. `DmaMemory` is
        /// page-backed and may be larger than the USB request itself.
        length: usize,
        buffer: DmaMemory,
    },
    SetConfiguration {
        value: u8,
    },
}

impl PendingControlTransfer {
    fn buffer(&self) -> Option<&DmaMemory> {
        match self {
            Self::DeviceDescriptor { buffer }
            | Self::ConfigurationHeader { buffer, .. }
            | Self::ConfigurationBody { buffer, .. } => Some(buffer),
            Self::SetConfiguration { .. } => None,
        }
    }

    fn length(&self) -> usize {
        match self {
            Self::DeviceDescriptor { .. } => USB_DEVICE_DESCRIPTOR_LENGTH,
            Self::ConfigurationHeader { .. } => 9,
            Self::ConfigurationBody { length, .. } => *length,
            Self::SetConfiguration { .. } => 0,
        }
    }

    fn configuration_index(&self) -> Option<u8> {
        match self {
            Self::ConfigurationHeader { index, .. } | Self::ConfigurationBody { index, .. } => {
                Some(*index)
            }
            Self::DeviceDescriptor { .. } => None,
            Self::SetConfiguration { .. } => None,
        }
    }
}

/// Work requested by a device after it consumes an IRQ event.
///
/// The host owns command-ring publication and MMIO doorbells; the device only
/// prepares its resources and returns this compact action.
pub(crate) enum DeviceAction {
    SubmitCommand(XhciCommand),
    RingEndpoint { dci: u8 },
    RecordInSys,
    Release,
}

/// DMA objects that must stay alive while the xHC owns a device slot.
pub struct XhciSlotResources {
    /// Input Context consumed by Address Device and later commands.
    pub input_context: DmaMemory,
    /// Output Device Context referenced by DCBAA[slot_id].
    pub output_context: DmaMemory,
    /// Default Control Endpoint transfer ring.
    pub ep0_ring: DmaMemory,
    /// One transfer ring for every endpoint added by Configure Endpoint.
    pub endpoint_rings: Vec<XhciEndpointRing>,
}

/// Transfer ring kept alive for one configured non-control endpoint.
pub struct XhciEndpointRing {
    /// xHCI Device Context Index associated with this ring.
    pub dci: u8,
    /// Page-backed transfer ring with a Link TRB in the last entry.
    pub ring: DmaMemory,
}

/// Minimal USB device record kept by the protocol layer.
///
/// Address Device is completed before descriptors are read, therefore VID and
/// PID remain optional while slot ID and device address become available.
pub struct XhciDevice {
    /// Host-local identity, valid before `slot_id` exists.
    pub(crate) id: DeviceId,
    /// One-based xHCI root-port number.
    pub port_id: u8,
    /// xHCI device slot assigned by Enable Slot.
    pub slot_id: Option<u8>,
    /// USB address selected by the xHC after Address Device.
    pub device_address: Option<u8>,
    /// Speed latched by PORTSC after reset.
    pub speed: PortSpeed,
    /// Current maximum packet size of the default control endpoint.
    pub ep0_max_packet_size: u16,
    /// Input/output contexts and EP0 ring owned by this device.
    pub contexts: Option<XhciSlotResources>,
    /// Context layout selected from HCCPARAMS1.CSZ for this controller.
    context_stride: usize,
    /// Current enumeration state.
    pub(crate) state: XhciDeviceState,
    /// Producer position and cycle state in the EP0 transfer ring.
    ep0_enqueue: usize,
    ep0_cycle: bool,
    /// The exact command or transfer currently awaited by this device.
    pending: Option<PendingOperation>,
    /// Full standard Device Descriptor, once enumeration has read it.
    descriptor: Option<DeviceDescriptor>,
    /// Parsed configurations; DMA backing pages are released after parsing.
    configurations: alloc::vec::Vec<UsbConfiguration>,
    /// Index of the next configuration descriptor to fetch.
    next_configuration: u8,
    /// Configuration selected by the USB SET_CONFIGURATION request.
    active_configuration: Option<u8>,
    /// Interface-driver claims owned by this device.
    drivers: DeviceidTable,
}

/// Shared handle passed to USB class drivers.
pub type XhciDeviceRef = Arc<NoIrqLock<XhciDevice>>;

/// Read-only USB view exported to future class drivers.
pub trait UsbDeviceView {
    fn device_descriptor(&self) -> Option<&DeviceDescriptor>;
    fn configurations(&self) -> &[UsbConfiguration];
    fn configuration(&self, value: u8) -> Option<&UsbConfiguration>;
    fn alternate_setting(
        &self,
        configuration_value: u8,
        interface: u8,
        alternate: u8,
    ) -> Option<&UsbAlternateSetting>;
}

impl XhciDevice {
    pub(crate) fn new(id: DeviceId, port_id: u8, speed: PortSpeed, context_stride: usize) -> Self {
        Self {
            id,
            port_id,
            slot_id: None,
            device_address: None,
            speed,
            ep0_max_packet_size: speed.ep0_max_packet_size(),
            contexts: None,
            context_stride,
            state: XhciDeviceState::SlotEnabling,
            ep0_enqueue: 0,
            ep0_cycle: true,
            pending: Some(PendingOperation::EnableSlot),
            descriptor: None,
            configurations: alloc::vec::Vec::new(),
            next_configuration: 0,
            active_configuration: None,
            drivers: DeviceidTable::default(),
        }
    }

    /// Consume matching global driver entries and claim this device's
    /// interfaces. The returned instances are probed after the device lock is
    /// released, so a driver may safely keep and use the device handle.
    pub(crate) fn try_find_and_register_driver(device: XhciDeviceRef) {
        let probes = device.lock(|device| {
            ensure_table_exist();
            let Some(descriptor) = device.descriptor else {
                return Vec::new();
            };
            let Some(configuration) = device
                .active_configuration
                .and_then(|value| device.configuration(value))
            else {
                return Vec::new();
            };
            let interfaces: Vec<_> = configuration
                .interfaces
                .iter()
                .cloned()
                .map(Arc::new)
                .collect();

            let mut probes = Vec::new();
            for interface in interfaces {
                let matcher = USBDRIERTABLE.lock(|table| {
                    let table = table.as_mut()?;
                    let index = table
                        .entries
                        .iter()
                        .position(|matcher| matcher.matches(descriptor, &interface))?;
                    Some(table.entries.remove(index))
                });
                let Some(matcher) = matcher else {
                    continue;
                };

                let driver = matcher.driver.new_driver();
                if device
                    .drivers
                    .register(Arc::clone(&interface), Arc::clone(&driver))
                    .is_ok()
                {
                    probes.push((driver, interface));
                }
            }
            probes
        });

        for (driver, interface) in probes {
            if let Err(reason) = driver.probe(Arc::clone(&device), interface) {
                error!("xhci: USB driver probe failed: {}", reason);
            }
        }
    }

    /// Build the small user-visible snapshot after descriptor enumeration.
    ///
    /// The snapshot intentionally copies identifiers only. Real descriptor
    /// and endpoint ownership remains with this xHCI device object.
    pub(crate) fn sys_record(&self, bus_number: u8) -> Option<UsbDeviceRecord> {
        if self.state != XhciDeviceState::Configured || self.active_configuration.is_none() {
            return None;
        }
        let descriptor = self.descriptor?;
        Some(UsbDeviceRecord {
            bus_number,
            device_number: self.device_address?,
            port_number: self.port_id,
            vendor_id: descriptor.vendor_id,
            product_id: descriptor.product_id,
            configuration_count: self.configurations.len() as u8,
        })
    }

    /// Convert a USB endpoint number and direction to an xHCI DCI.
    ///
    /// DCI 1 is the default control endpoint. For every other endpoint,
    /// xHCI uses `2 * endpoint_number + direction`, where OUT is 0 and IN is
    /// 1. The endpoint number is four bits wide in USB, so 0..=15 is valid.
    pub(crate) fn endpoint_to_dci(endpoint_number: u8, direction_in: bool) -> Option<u8> {
        if endpoint_number > MAX_ENDPOINT_NUMBER {
            return None;
        }
        if endpoint_number == 0 {
            return Some(1);
        }
        Some(endpoint_number * 2 + u8::from(direction_in))
    }

    /// Consume a Command Completion already routed by its command-ring slot.
    pub(crate) fn on_command_completion(
        &mut self,
        command: CommandCompletionEvent,
        max_slots: u8,
        dcbaa: &DmaMemory,
    ) -> Result<DeviceAction, &'static str> {
        if command.completion_code != CompletionCode::Success {
            return Err("xHCI device command did not complete successfully");
        }
        match self.pending.take() {
            Some(PendingOperation::EnableSlot) => {
                if command.slot_id == 0 || command.slot_id > max_slots {
                    return Err("xHCI Enable Slot returned an invalid slot");
                }
                let command = self.prepare_address_device_command(command.slot_id, dcbaa)?;
                self.state = XhciDeviceState::Addressing;
                self.pending = Some(PendingOperation::AddressDevice);
                Ok(DeviceAction::SubmitCommand(command))
            }
            Some(PendingOperation::AddressDevice) => {
                if self.slot_id != Some(command.slot_id) {
                    return Err("xHCI Address Device slot does not match the device");
                }
                let address = self.finish_address_device()?;
                info!(
                    "xhci: device addressed: port={}, slot={}, device_address={}",
                    self.port_id, command.slot_id, address
                );
                self.state = XhciDeviceState::Enumerating;
                self.begin_device_descriptor()
            }
            Some(PendingOperation::EvaluateContext) => {
                if self.slot_id != Some(command.slot_id) {
                    return Err("xHCI Evaluate Context slot does not match the device");
                }
                self.state = XhciDeviceState::Enumerating;
                self.begin_configuration_header(0)
            }
            Some(PendingOperation::ConfigureEndpoint {
                configuration_value,
            }) => {
                if self.slot_id != Some(command.slot_id) {
                    return Err("xHCI Configure Endpoint slot does not match the device");
                }
                info!(
                    "xhci: Configure Endpoint succeeded: port={}, slot={}, configuration={}",
                    self.port_id, command.slot_id, configuration_value
                );
                self.debug_desc(configuration_value);
                self.state = XhciDeviceState::SettingConfiguration;
                self.submit_control(PendingControlTransfer::SetConfiguration {
                    value: configuration_value,
                })
            }
            Some(PendingOperation::DisableSlot) => {
                if self.slot_id != Some(command.slot_id) {
                    return Err("xHCI Disable Slot ID does not match the device");
                }
                self.finish_disable_slot(dcbaa)?;
                Ok(DeviceAction::Release)
            }
            Some(operation) => {
                self.pending = Some(operation);
                Err("xHCI command completion does not match pending operation")
            }
            None => Err("xHCI device has no pending command"),
        }
    }

    /// Consume a Transfer Event for this slot after the host routes it by slot ID.
    pub(crate) fn on_transfer_completion(
        &mut self,
        event: TransferEvent,
    ) -> Result<DeviceAction, &'static str> {
        if event.completion_code != CompletionCode::Success {
            return Err("xHCI control transfer failed");
        }
        if event.slot_id != self.slot_id.unwrap_or(0) || event.endpoint_id != EP0_DCI {
            return Err("xHCI transfer event does not match device EP0");
        }
        let PendingOperation::Control {
            transfer,
            status_trb_pointer,
        } = self
            .pending
            .take()
            .ok_or("xHCI device has no pending transfer")?
        else {
            return Err("xHCI transfer event does not match pending operation");
        };
        if event.trb_pointer != status_trb_pointer {
            self.pending = Some(PendingOperation::Control {
                transfer,
                status_trb_pointer,
            });
            return Err("xHCI transfer event does not match pending TRB");
        }
        self.finish_control_transfer(transfer)
    }

    /// Begin the slot shutdown required after a disconnect notification.
    pub(crate) fn disable_slot(&mut self) -> Result<XhciCommand, &'static str> {
        let slot_id = self.slot_id.ok_or("xHCI device has no slot to disable")?;
        if self.state == XhciDeviceState::Disconnecting {
            return Err("xHCI device Disable Slot is already pending");
        }
        self.state = XhciDeviceState::Disconnecting;
        self.pending = Some(PendingOperation::DisableSlot);
        Ok(XhciCommand::DisableSlot { slot_id })
    }

    fn begin_device_descriptor(&mut self) -> Result<DeviceAction, &'static str> {
        let buffer = DmaMemory::new(DMA_PAGE_SIZE).ok_or("cannot allocate device descriptor")?;
        self.submit_control(PendingControlTransfer::DeviceDescriptor { buffer })
    }

    fn begin_configuration_header(&mut self, index: u8) -> Result<DeviceAction, &'static str> {
        let buffer = DmaMemory::new(DMA_PAGE_SIZE).ok_or("cannot allocate configuration header")?;
        self.submit_control(PendingControlTransfer::ConfigurationHeader { index, buffer })
    }

    /// Build and publish one descriptor request on EP0. The returned action
    /// lets the host ring the device doorbell after the device lock is released.
    fn submit_control(
        &mut self,
        transfer: PendingControlTransfer,
    ) -> Result<DeviceAction, &'static str> {
        let is_set_configuration =
            matches!(&transfer, PendingControlTransfer::SetConfiguration { .. });
        if is_set_configuration {
            if self.state != XhciDeviceState::SettingConfiguration {
                return Err("xHCI device is not waiting for SET_CONFIGURATION");
            }
        } else if self.state != XhciDeviceState::Enumerating {
            return Err("xHCI device is not addressed");
        }

        let (setup, data) = match &transfer {
            PendingControlTransfer::SetConfiguration { value } => (
                UsbSetup {
                    bm_request_type: 0x00,
                    b_request: USB_REQUEST_SET_CONFIGURATION,
                    w_value: u16::from(*value),
                    w_index: 0,
                    w_length: 0,
                },
                None,
            ),
            _ => {
                let (descriptor_type, descriptor_index) = match transfer.configuration_index() {
                    Some(index) => (USB_DESCRIPTOR_CONFIGURATION, index),
                    None => (USB_DESCRIPTOR_DEVICE, 0),
                };
                let buffer = transfer
                    .buffer()
                    .ok_or("descriptor transfer has no DMA buffer")?;
                let length = transfer.length();
                if length > buffer.len() {
                    return Err("descriptor buffer is too small");
                }
                buffer.invalidate_for_cpu();
                let setup = UsbSetup {
                    bm_request_type: 0x80,
                    b_request: USB_REQUEST_GET_DESCRIPTOR,
                    w_value: (descriptor_type << 8) | u16::from(descriptor_index),
                    w_index: 0,
                    w_length: length as u16,
                };
                let data = UsbDataStage {
                    buffer: buffer.phys_addr() as u64,
                    length: length as u32,
                    direction_in: true,
                    interrupt_on_short_packet: true,
                };
                (setup, Some(data))
            }
        };

        let cycle = self.ep0_cycle;
        let mut trbs = [Trb::default(); 3];
        trbs[0] = setup.to_trb(cycle);
        let trb_count = if let Some(data) = data {
            trbs[1] = data.to_trb(cycle);
            trbs[2] = setup.status_stage().to_trb(cycle);
            3
        } else {
            trbs[1] = setup.status_stage().to_trb(cycle);
            2
        };
        let status_trb_pointer = self.push_ep0_transfer(&trbs[..trb_count])?;
        self.pending = Some(PendingOperation::Control {
            transfer,
            status_trb_pointer,
        });
        Ok(DeviceAction::RingEndpoint { dci: EP0_DCI })
    }

    /// Append Setup/Data/Status TRBs. The final Status TRB identifies the
    /// transfer directly, so internal enumeration needs no global waiter.
    fn push_ep0_transfer(&mut self, trbs: &[Trb]) -> Result<u64, &'static str> {
        if trbs.is_empty() {
            return Err("xHCI control transfer has no TRBs");
        }
        let enqueue = self.ep0_enqueue;
        if enqueue + trbs.len() > TRBS_PER_RING - 1 {
            return Err("xHCI EP0 transfer ring is full");
        }
        let contexts = self
            .contexts
            .as_ref()
            .ok_or("xHCI device has no EP0 context")?;
        let ring = &contexts.ep0_ring;
        unsafe {
            for (offset, trb) in trbs.iter().copied().enumerate() {
                write_volatile(ring.as_ptr::<Trb>((enqueue + offset) * TRB_SIZE), trb);
            }
        }
        ring.clean_for_device();
        self.ep0_enqueue += trbs.len();
        Ok((ring.phys_addr() + (self.ep0_enqueue - 1) * TRB_SIZE) as u64)
    }

    fn finish_control_transfer(
        &mut self,
        transfer: PendingControlTransfer,
    ) -> Result<DeviceAction, &'static str> {
        if let Some(buffer) = transfer.buffer() {
            buffer.invalidate_for_cpu();
        }
        match transfer {
            PendingControlTransfer::DeviceDescriptor { buffer } => {
                let bytes = dma_bytes(&buffer, USB_DEVICE_DESCRIPTOR_LENGTH);
                let descriptor = DeviceDescriptor::parse(&bytes)
                    .map_err(|_| "xHCI returned an invalid USB device descriptor")?;
                let ep0_max_packet_size =
                    descriptor_ep0_max_packet_size(self.speed, descriptor.max_packet_size0)
                        .ok_or("USB device reports an invalid EP0 maximum packet size")?;
                info!(
                    "xhci: device descriptor: port={}, slot={}, address={}, ep0_mps={}, vid={:#06x}, pid={:#06x}",
                    self.port_id,
                    self.slot_id.unwrap_or(0),
                    self.device_address.unwrap_or(0),
                    ep0_max_packet_size,
                    descriptor.vendor_id,
                    descriptor.product_id,
                );
                self.descriptor = Some(descriptor);
                self.next_configuration = 0;
                if descriptor.configuration_count == 0 {
                    return Err("USB device has no configuration descriptor");
                }
                // The first request used the speed default. Publish the
                // descriptor's actual EP0 MPS before any more control TDs.
                self.ep0_max_packet_size = ep0_max_packet_size;
                self.prepare_ep0_context_update()
            }
            PendingControlTransfer::ConfigurationHeader { index, buffer } => {
                let header = dma_bytes(&buffer, 9);
                let descriptor = super::descriptor::ConfigurationDescriptor::parse(&header)
                    .map_err(|_| "xHCI returned an invalid configuration header")?;
                let total = usize::from(descriptor.total_length);
                if total < 9 {
                    return Err("xHCI configuration descriptor has invalid total length");
                }
                let buffer =
                    DmaMemory::new(total).ok_or("cannot allocate configuration descriptor")?;
                self.submit_control(PendingControlTransfer::ConfigurationBody {
                    index,
                    length: total,
                    buffer,
                })
            }
            PendingControlTransfer::ConfigurationBody {
                index,
                length,
                buffer,
            } => {
                let bytes = dma_bytes(&buffer, length);
                let configuration = UsbConfiguration::parse(&bytes)
                    .map_err(|_| "xHCI returned an invalid configuration descriptor")?;
                if configuration.descriptor.value == 0 || index != self.next_configuration {
                    return Err("xHCI configuration descriptor arrived out of order");
                }
                self.configurations.push(configuration);
                self.next_configuration = self.next_configuration.saturating_add(1);
                let expected = self
                    .descriptor
                    .ok_or("xHCI device descriptor is missing")?
                    .configuration_count;
                if self.next_configuration < expected {
                    self.begin_configuration_header(self.next_configuration)
                } else {
                    info!(
                        "xhci: descriptors ready: port={}, slot={}, configurations={}",
                        self.port_id,
                        self.slot_id.unwrap_or(0),
                        self.configurations.len(),
                    );
                    self.prepare_configure_endpoint_command()
                }
            }
            PendingControlTransfer::SetConfiguration { value } => {
                if self.configuration(value).is_none() {
                    return Err("xHCI SET_CONFIGURATION selected an unknown configuration");
                }
                self.active_configuration = Some(value);
                self.state = XhciDeviceState::Configured;
                info!(
                    "xhci: USB configuration active: port={}, slot={}, value={}",
                    self.port_id,
                    self.slot_id.unwrap_or(0),
                    value,
                );
                Ok(DeviceAction::RecordInSys)
            }
        }
    }

    /// Update EP0 with the value found in the Device Descriptor.
    ///
    /// Address Device used a speed-based initial MPS. USB 2.0 devices then
    /// report their exact EP0 MPS in bMaxPacketSize0, so xHCI requires an
    /// Evaluate Context command before another control transfer is issued.
    fn prepare_ep0_context_update(&mut self) -> Result<DeviceAction, &'static str> {
        if self.state != XhciDeviceState::Enumerating {
            return Err("xHCI device is not ready to update EP0");
        }
        let slot_id = self.slot_id.ok_or("xHCI device has no slot")?;
        let input_context =
            DmaMemory::new(DMA_PAGE_SIZE).ok_or("cannot allocate EP0 Input Context")?;
        zero_dma(&input_context);
        let context_stride = self.context_stride;
        let ep0_max_packet_size = self.ep0_max_packet_size;
        let input_ep0_context_offset = context_stride * 2;
        let output_ep0_context_offset = context_stride;

        let contexts = self
            .contexts
            .as_mut()
            .ok_or("xHCI device has no slot contexts")?;
        contexts.output_context.invalidate_for_cpu();
        unsafe {
            // Evaluate Context uses A1 and a complete EP0 context. Preserve
            // the controller's dequeue/state fields and replace only MPS.
            for offset in (0..context_stride).step_by(core::mem::size_of::<u32>()) {
                let value = read_volatile(
                    contexts
                        .output_context
                        .as_ptr::<u32>(output_ep0_context_offset + offset),
                );
                write_context_u32(&input_context, input_ep0_context_offset + offset, value);
            }
            let endpoint_dword1 = read_volatile(
                input_context
                    .as_ptr::<u32>(input_ep0_context_offset + ENDPOINT_CONTEXT_DWORD1_OFFSET),
            );
            let endpoint_dword1 = (endpoint_dword1 & !ENDPOINT_CONTEXT_MAX_PACKET_SIZE_MASK)
                | (u32::from(ep0_max_packet_size) << ENDPOINT_CONTEXT_MAX_PACKET_SIZE_SHIFT);
            write_context_u32(
                &input_context,
                input_ep0_context_offset + ENDPOINT_CONTEXT_DWORD1_OFFSET,
                endpoint_dword1,
            );
            // Input Control Context: A1=1, while Slot and Drop flags stay 0.
            write_context_u32(&input_context, INPUT_CONTROL_CONTEXT_DROP_CONTEXT_OFFSET, 0);
            write_context_u32(
                &input_context,
                INPUT_CONTROL_CONTEXT_ADD_CONTEXT_OFFSET,
                INPUT_CONTROL_CONTEXT_ADD_EP0,
            );
        }
        input_context.clean_for_device();
        let input_context_pointer = input_context.phys_addr() as u64;
        contexts.input_context = input_context;
        self.pending = Some(PendingOperation::EvaluateContext);
        self.state = XhciDeviceState::UpdatingEndpoint0;
        Ok(DeviceAction::SubmitCommand(XhciCommand::EvaluateContext {
            slot_id,
            input_context_pointer,
        }))
    }

    /// Build the Input Context for the first USB configuration.
    ///
    /// USB selects alternate setting zero when SET_CONFIGURATION is issued.
    /// Therefore this pass adds every endpoint described by alternate zero of
    /// every interface. The endpoint rings are retained with the device so
    /// their physical addresses remain valid after the command completes.
    fn prepare_configure_endpoint_command(&mut self) -> Result<DeviceAction, &'static str> {
        if self.state != XhciDeviceState::Enumerating {
            return Err("xHCI device is not ready to configure endpoints");
        }
        let configuration = self
            .configurations
            .first()
            .ok_or("USB device has no configuration descriptor")?;
        let configuration_value = configuration.descriptor.value;

        let mut endpoints = Vec::new();
        for interface in &configuration.interfaces {
            let alternate = interface
                .alternate_setting(0)
                .ok_or("USB interface has no alternate setting zero")?;
            endpoints.extend(alternate.endpoints.iter().copied());
        }

        let mut endpoint_rings = Vec::with_capacity(endpoints.len());
        let mut add_context_flags = INPUT_CONTROL_CONTEXT_ADD_SLOT;
        let mut max_dci = EP0_DCI;
        for endpoint in endpoints.iter().copied() {
            let direction_in = matches!(endpoint.address.direction, EndpointDirection::In);
            let dci = Self::endpoint_to_dci(endpoint.address.number, direction_in)
                .ok_or("USB endpoint number cannot be represented by xHCI")?;
            if dci == EP0_DCI {
                return Err("USB configuration contains an invalid endpoint zero");
            }
            if endpoint_rings
                .iter()
                .any(|ring: &XhciEndpointRing| ring.dci == dci)
            {
                return Err("USB configuration contains duplicate endpoint DCI");
            }

            let ring = DmaMemory::new(DMA_PAGE_SIZE)
                .ok_or("cannot allocate xHCI endpoint transfer ring")?;
            zero_dma(&ring);
            unsafe {
                // The final entry is a Link TRB back to the ring base. Its
                // Toggle Cycle bit makes the producer cycle wrap correctly.
                write_volatile(
                    ring.as_ptr::<Trb>((TRBS_PER_RING - 1) * TRB_SIZE),
                    Trb {
                        parameter: ring.phys_addr() as u64,
                        status: 0,
                        control: TRB_TYPE_LINK | TRB_LINK_TOGGLE_CYCLE | TRB_CYCLE,
                    },
                );
            }
            ring.clean_for_device();
            add_context_flags |= 1_u32 << dci;
            max_dci = max_dci.max(dci);
            endpoint_rings.push(XhciEndpointRing { dci, ring });
        }

        let input_context = DmaMemory::new(DMA_PAGE_SIZE)
            .ok_or("cannot allocate Configure Endpoint Input Context")?;
        zero_dma(&input_context);
        let context_stride = self.context_stride;
        let speed = self.speed;
        let input_slot_context_offset = context_stride;
        let speed_code = speed.context_code();

        let contexts = self
            .contexts
            .as_mut()
            .ok_or("xHCI device has no slot contexts")?;
        // Input Context (xHCI 6.2.5, one 4 KiB page)
        // ┌────────────────────────────┐
        // │ Input Control Context 00h  │ A0 + endpoint DCI flags
        // ├────────────────────────────┤
        // │ Input Slot Context     20h │ speed, root port, entries=max DCI
        // ├────────────────────────────┤
        // │ Input Endpoint 0       40h │ ignored by Configure Endpoint
        // ├────────────────────────────┤
        // │ Input DCI 2, 3, ...        │ one context per added endpoint
        // └────────────────────────────┘
        // With CSZ=1, the 20h/40h offsets become 40h/80h; the code below
        // uses context_stride so both layouts encode the same indices.
        unsafe {
            // Input Slot Context fields that are input-owned. Device Address
            // and Slot State stay zero; the xHC owns those output fields.
            let slot_dword0 = (speed_code << SLOT_CONTEXT_SPEED_SHIFT)
                | (u32::from(max_dci) << SLOT_CONTEXT_CONTEXT_ENTRIES_SHIFT);
            write_context_u32(
                &input_context,
                input_slot_context_offset + SLOT_CONTEXT_DWORD0_OFFSET,
                slot_dword0,
            );
            write_context_u32(
                &input_context,
                input_slot_context_offset + SLOT_CONTEXT_DWORD1_OFFSET,
                u32::from(self.port_id) << SLOT_CONTEXT_ROOT_PORT_SHIFT,
            );
            // This is a directly attached root-port device, so Hub=0 and the
            // hub-only Number of Ports/TT fields remain zero.
            // Configure Endpoint uses A0 plus the endpoint DCI bits. EP0 is
            // already configured by Address Device and is not re-added.
            write_context_u32(&input_context, INPUT_CONTROL_CONTEXT_DROP_CONTEXT_OFFSET, 0);
            write_context_u32(
                &input_context,
                INPUT_CONTROL_CONTEXT_ADD_CONTEXT_OFFSET,
                add_context_flags,
            );

            for (endpoint, endpoint_ring) in endpoints.iter().zip(&endpoint_rings) {
                let endpoint_context_offset = context_stride * (1 + usize::from(endpoint_ring.dci));
                let payload = endpoint.max_packet_payload();
                if payload == 0 {
                    return Err("USB endpoint has a zero maximum packet size");
                }
                let endpoint_type = endpoint_type_code(*endpoint);
                let cerr = if matches!(endpoint.transfer_type, EndpointTransferType::Isochronous) {
                    0
                } else {
                    ENDPOINT_CONTEXT_CERR
                };
                let periodic = matches!(
                    endpoint.transfer_type,
                    EndpointTransferType::Isochronous | EndpointTransferType::Interrupt
                );
                let transactions = if speed == PortSpeed::HighSpeed && periodic {
                    u32::from(endpoint.max_packet_transactions())
                } else {
                    1
                };
                // For high-speed periodic endpoints USB wMaxPacketSize[12:11]
                // is xHCI Max Burst Size (additional transaction slots).
                // Mult is reserved/zero for this USB 2.0 case.
                let max_burst = transactions.saturating_sub(1).min(3);
                let mult = 0;
                let interval = if periodic {
                    u32::from(xhci_interval(speed, *endpoint))
                } else {
                    0
                };
                let max_esit_payload = if periodic {
                    u32::from(payload) * (max_burst + 1) * (mult + 1)
                } else {
                    0
                };

                // Input Endpoint Context (xHCI 6.2.3): EP starts stopped,
                // DCS=1 points at an empty ring, and no streams are enabled.
                write_context_u32(
                    &input_context,
                    endpoint_context_offset + ENDPOINT_CONTEXT_DWORD0_OFFSET,
                    (mult << ENDPOINT_CONTEXT_MULT_SHIFT)
                        | (interval << ENDPOINT_CONTEXT_INTERVAL_SHIFT)
                        | ((max_esit_payload >> 16)
                            << ENDPOINT_CONTEXT_MAX_ESIT_PAYLOAD_HIGH_SHIFT),
                );
                write_context_u32(
                    &input_context,
                    endpoint_context_offset + ENDPOINT_CONTEXT_DWORD1_OFFSET,
                    cerr | (endpoint_type << ENDPOINT_CONTEXT_EP_TYPE_SHIFT)
                        | (max_burst << ENDPOINT_CONTEXT_MAX_BURST_SHIFT)
                        | (u32::from(payload) << ENDPOINT_CONTEXT_MAX_PACKET_SIZE_SHIFT),
                );
                write_context_u32(
                    &input_context,
                    endpoint_context_offset + ENDPOINT_CONTEXT_TR_DEQUEUE_LO_OFFSET,
                    (endpoint_ring.ring.phys_addr() as u32) | ENDPOINT_CONTEXT_DCS,
                );
                write_context_u32(
                    &input_context,
                    endpoint_context_offset + ENDPOINT_CONTEXT_TR_DEQUEUE_HI_OFFSET,
                    (endpoint_ring.ring.phys_addr() >> 32) as u32,
                );
                write_context_u32(
                    &input_context,
                    endpoint_context_offset + ENDPOINT_CONTEXT_DWORD4_OFFSET,
                    ENDPOINT_CONTEXT_AVERAGE_TRB_LENGTH
                        | ((max_esit_payload & 0xffff)
                            << ENDPOINT_CONTEXT_MAX_ESIT_PAYLOAD_LOW_SHIFT),
                );
            }
        }

        input_context.clean_for_device();
        let slot_id = self.slot_id.ok_or("xHCI device has no slot")?;
        let input_context_pointer = input_context.phys_addr() as u64;
        contexts.input_context = input_context;
        contexts.endpoint_rings = endpoint_rings;
        self.pending = Some(PendingOperation::ConfigureEndpoint {
            configuration_value,
        });
        self.state = XhciDeviceState::ConfiguringEndpoints;
        Ok(DeviceAction::SubmitCommand(
            XhciCommand::ConfigureEndpoint {
                slot_id,
                input_context_pointer,
            },
        ))
    }

    /// Print the active configuration as a small descriptor tree.
    fn debug_desc(&self, configuration_value: u8) {
        let Some(configuration) = self.configuration(configuration_value) else {
            return;
        };
        info!(
            "xhci: descriptor tree: configuration value={}, interfaces={}, total_length={}",
            configuration.descriptor.value,
            configuration.interfaces.len(),
            configuration.descriptor.total_length,
        );
        for interface in &configuration.interfaces {
            info!(
                "  interface {} ({} alternate settings)",
                interface.number,
                interface.alternate_settings.len()
            );
            for alternate in &interface.alternate_settings {
                let selected = if alternate.descriptor.alternate_setting == 0 {
                    " [active]"
                } else {
                    ""
                };
                info!(
                    "    alternate {}{}: class={:02x}/{:02x}/{:02x}, endpoints={}",
                    alternate.descriptor.alternate_setting,
                    selected,
                    alternate.descriptor.class.class,
                    alternate.descriptor.class.subclass,
                    alternate.descriptor.class.protocol,
                    alternate.endpoints.len(),
                );
                for endpoint in &alternate.endpoints {
                    let direction_in = matches!(endpoint.address.direction, EndpointDirection::In);
                    let dci = Self::endpoint_to_dci(endpoint.address.number, direction_in);
                    info!(
                        "      endpoint 0x{:02x}: dci={:?}, direction={:?}, type={:?}, mps={}, transactions={}, interval={}",
                        endpoint.address.number | if direction_in { 0x80 } else { 0 },
                        dci,
                        endpoint.address.direction,
                        endpoint.transfer_type,
                        endpoint.max_packet_payload(),
                        endpoint.max_packet_transactions(),
                        endpoint.interval,
                    );
                }
            }
        }
    }

    /// Release the slot-owned context only after Disable Slot completes.
    fn finish_disable_slot(&mut self, dcbaa: &DmaMemory) -> Result<(), &'static str> {
        let slot_id = self.slot_id.take().ok_or("xHCI device has no slot")?;
        unsafe {
            write_volatile(dcbaa.as_ptr::<u64>(usize::from(slot_id) * 8), 0);
        }
        dcbaa.clean_for_device();
        self.pending = None;
        self.contexts = None;
        self.device_address = None;
        self.state = XhciDeviceState::Dead;
        Ok(())
    }
    /// Allocate the pages required by Address Device and initialize them.
    ///
    /// Input Context contains Input Control, Slot, and Endpoint 0 contexts.
    /// The Output Context starts zeroed and is owned by the xHC after the
    /// Address Device command. EP0 gets an empty transfer ring with a Link
    /// TRB, and its dequeue pointer starts with DCS=1.
    fn prepare_address_device_command(
        &mut self,
        slot_id: u8,
        dcbaa: &DmaMemory,
    ) -> Result<XhciCommand, &'static str> {
        if self.state != XhciDeviceState::SlotEnabling || slot_id == 0 {
            return Err("xHCI device is not waiting for Enable Slot");
        }

        let input_context = DmaMemory::new(DMA_PAGE_SIZE).ok_or("cannot allocate Input Context")?;
        let output_context =
            DmaMemory::new(DMA_PAGE_SIZE).ok_or("cannot allocate Output Context")?;
        let ep0_ring = DmaMemory::new(DMA_PAGE_SIZE).ok_or("cannot allocate EP0 ring")?;
        zero_dma(&input_context);
        zero_dma(&output_context);
        zero_dma(&ep0_ring);

        let input_slot_context_offset = self.context_stride;
        let input_ep0_context_offset = self.context_stride * 2;
        let ep0_max_packet_size = u32::from(self.speed.ep0_max_packet_size());

        unsafe {
            // Input Control Context: add Slot (A0) and Default Control
            // Endpoint (A1); all Drop Context and other Add Context bits 0.
            write_context_u32(&input_context, INPUT_CONTROL_CONTEXT_DROP_CONTEXT_OFFSET, 0);
            write_context_u32(
                &input_context,
                INPUT_CONTROL_CONTEXT_ADD_CONTEXT_OFFSET,
                INPUT_CONTROL_CONTEXT_ADD_SLOT | INPUT_CONTROL_CONTEXT_ADD_EP0,
            );

            // Directly attached devices use route string 0. Context Entries=1
            // means that only the default control endpoint is valid.
            write_context_u32(
                &input_context,
                input_slot_context_offset + SLOT_CONTEXT_DWORD0_OFFSET,
                (self.speed.context_code() << SLOT_CONTEXT_SPEED_SHIFT)
                    | SLOT_CONTEXT_INITIAL_CONTEXT_ENTRIES,
            );
            write_context_u32(
                &input_context,
                input_slot_context_offset + SLOT_CONTEXT_DWORD1_OFFSET,
                u32::from(self.port_id) << SLOT_CONTEXT_ROOT_PORT_SHIFT,
            );

            // Endpoint Context DWORD0: EP State, Mult, streams and LSA stay
            // zero. EP0 has no interval or ESIT payload requirement.
            write_context_u32(
                &input_context,
                input_ep0_context_offset + ENDPOINT_CONTEXT_DWORD0_OFFSET,
                0,
            );
            // Endpoint Context DWORD1: CErr=3 and EP Type=Control. Interval,
            // Max Burst and HID remain zero; Max Packet Size is required.
            write_context_u32(
                &input_context,
                input_ep0_context_offset + ENDPOINT_CONTEXT_DWORD1_OFFSET,
                ENDPOINT_CONTEXT_CERR
                    | ENDPOINT_CONTEXT_EP_TYPE_CONTROL
                    | (ep0_max_packet_size << ENDPOINT_CONTEXT_MAX_PACKET_SIZE_SHIFT),
            );
            write_context_u32(
                &input_context,
                input_ep0_context_offset + ENDPOINT_CONTEXT_TR_DEQUEUE_LO_OFFSET,
                (ep0_ring.phys_addr() as u32) | ENDPOINT_CONTEXT_DCS,
            );
            write_context_u32(
                &input_context,
                input_ep0_context_offset + ENDPOINT_CONTEXT_TR_DEQUEUE_HI_OFFSET,
                (ep0_ring.phys_addr() >> 32) as u32,
            );

            // A transfer ring needs a Link TRB at its final entry. The first
            // TRB remains zero until a control transfer is submitted.
            write_volatile(
                ep0_ring.as_ptr::<super::Trb>((TRBS_PER_RING - 1) * TRB_SIZE),
                super::Trb {
                    parameter: ep0_ring.phys_addr() as u64,
                    status: 0,
                    control: TRB_TYPE_LINK | TRB_LINK_TOGGLE_CYCLE | TRB_CYCLE,
                },
            );
        }

        // Input Context (CSZ=0, 32-byte contexts)
        // ┌────────────────────────────┐
        // │ Input Control Context 00h  │ A0/A1 = 1
        // ├────────────────────────────┤
        // │ Input Slot Context     20h │ root port, speed, entries=1
        // ├────────────────────────────┤
        // │ Input Endpoint 0       40h │ control EP, MPS, TR dequeue
        // └────────────────────────────┘
        // CSZ=1 uses 40h/80h for the last two rows; one 4 KiB page is enough.
        // The output page is already zeroed by the DMA allocator. Clean all
        // software-owned pages before the controller can fetch them.
        input_context.clean_for_device();
        output_context.clean_for_device();
        ep0_ring.clean_for_device();

        let input_context_pointer = input_context.phys_addr() as u64;
        let output_context_pointer = output_context.phys_addr() as u64;
        unsafe {
            // DCBAA[0] is reserved; the selected slot owns this output page.
            write_volatile(
                dcbaa.as_ptr::<u64>(slot_id as usize * 8),
                output_context_pointer,
            );
        }
        dcbaa.clean_for_device();

        self.slot_id = Some(slot_id);
        self.contexts = Some(XhciSlotResources {
            input_context,
            output_context,
            ep0_ring,
            endpoint_rings: Vec::new(),
        });
        Ok(XhciCommand::AddressDevice {
            slot_id,
            input_context_pointer,
        })
    }

    fn finish_address_device(&mut self) -> Result<u8, &'static str> {
        if self.state != XhciDeviceState::Addressing {
            return Err("xHCI device is not waiting for Address Device");
        }
        let contexts = self
            .contexts
            .as_ref()
            .ok_or("xHCI device has no output context")?;
        contexts.output_context.invalidate_for_cpu();
        let slot_context_dword3 =
            unsafe { read_volatile(contexts.output_context.as_ptr::<u32>(0x0c)) };
        let device_address = (slot_context_dword3 & 0xff) as u8;
        if device_address == 0 {
            return Err("xHCI Address Device returned address zero");
        }
        self.device_address = Some(device_address);
        Ok(device_address)
    }
}

impl UsbDeviceView for XhciDevice {
    fn device_descriptor(&self) -> Option<&DeviceDescriptor> {
        self.descriptor.as_ref()
    }

    fn configurations(&self) -> &[UsbConfiguration] {
        &self.configurations
    }

    fn configuration(&self, value: u8) -> Option<&UsbConfiguration> {
        self.configurations
            .iter()
            .find(|configuration| configuration.descriptor.value == value)
    }

    fn alternate_setting(
        &self,
        configuration_value: u8,
        interface: u8,
        alternate: u8,
    ) -> Option<&UsbAlternateSetting> {
        self.configuration(configuration_value)?
            .alternate_setting(interface, alternate)
    }
}

impl XhciHost {
    /// Read one root-port PORTSC without acknowledging its change latches.
    pub(crate) fn read_port_status(&self, port_id: u8) -> Option<PortStatusChangeEvent> {
        if port_id == 0 || port_id > self.max_ports {
            error!(
                "xhci: port-status event contains invalid port {} (max {})",
                port_id, self.max_ports
            );
            return None;
        }

        let portsc = self.op_read32(portsc_offset(port_id));
        let event = PortStatusChangeEvent::from_portsc(port_id, portsc);
        event.has_change().then_some(event)
    }

    /// Reset a connected, powered root port and retain its negotiated speed.
    ///
    /// The write is a PORTSC write-neutral value: all RW1CS bits are zero,
    /// while current PLS/PP/PIC/wake-enable and read-only status are kept.
    /// This prevents a reset request from accidentally clearing CSC or
    /// powering the port off.
    pub(crate) fn reset_port(
        &mut self,
        port_id: u8,
    ) -> Result<PortStatusChangeEvent, &'static str> {
        if port_id == 0 || port_id > self.max_ports {
            return Err("xHCI port number is invalid");
        }

        let offset = portsc_offset(port_id);
        let portsc = self.op_read32(offset);
        if portsc & PORTSC_CCS == 0 {
            self.remove_device(port_id);
            return Err("xHCI port has no connected device");
        }
        if portsc & PORTSC_PP == 0 {
            return Err("xHCI connected port is not powered");
        }

        let status = PortStatusChangeEvent::from_portsc(port_id, portsc);
        let warm_reset = matches!(
            status.link_state,
            PortLinkState::Inactive | PortLinkState::Compliance
        );
        let reset_bit = if warm_reset { PORTSC_WPR } else { PORTSC_PR };

        // Do not let a previous reset-complete latch make the next reset
        // appear complete immediately. This write clears only stale PRC/WRC;
        // CSC and every unrelated change latch remain untouched.
        let stale_reset_changes = portsc & (PORTSC_PRC | PORTSC_WRC);
        if stale_reset_changes != 0 {
            self.op_write32(offset, portsc_neutral_value(portsc, stale_reset_changes));
        }

        let portsc = self.op_read32(offset);
        let reset_value = portsc_neutral_value(portsc, reset_bit);

        info!(
            "xhci: port {} reset: mode={}, portsc={:#010x}",
            port_id,
            if warm_reset { "warm" } else { "standard" },
            reset_value
        );
        self.op_write32(offset, reset_value);

        self.wait_until(
            |host| {
                let value = host.op_read32(offset);
                let reset_done = value & reset_bit == 0;
                let completion = if warm_reset { PORTSC_WRC } else { PORTSC_PRC };
                reset_done && value & completion != 0
            },
            if warm_reset {
                "xHCI warm port reset"
            } else {
                "xHCI port reset"
            },
        )?;

        let after_reset = PortStatusChangeEvent::from_portsc(port_id, self.op_read32(offset));
        if !after_reset.ccs || !after_reset.powered {
            self.remove_device(port_id);
            return Err("xHCI device disconnected during port reset");
        }

        if after_reset.speed == PortSpeed::SuperSpeed {
            if after_reset.link_state != PortLinkState::U0 {
                return Err("xHCI SuperSpeed port did not reach U0");
            }
        } else if !after_reset.ped {
            return Err("xHCI USB2 port was not enabled by reset");
        }

        // The reset request above deliberately wrote all RW1CS bits as zero,
        // so CSC survived long enough to describe the connection event. Now
        // that the event has been logged and reset has completed, acknowledge
        // both CSC and the reset-complete latch. Otherwise QEMU (and compliant
        // controllers) can keep reporting the same port change forever.
        let completion = if warm_reset { PORTSC_WRC } else { PORTSC_PRC };
        let mut handled_changes = completion;
        if after_reset.ccs && after_reset.csc {
            handled_changes |= PORTSC_CSC;
        }
        self.op_write32(
            offset,
            portsc_neutral_value(after_reset.portsc, handled_changes),
        );
        self.wait_reset_recovery();

        info!(
            "xhci: port {} reset complete: speed={:?}, ped={}, pls={:?}",
            port_id, after_reset.speed, after_reset.ped, after_reset.link_state
        );
        Ok(after_reset)
    }

    fn wait_reset_recovery(&self) {
        // USB 2.0 requires T_RSTRCY >= 10 ms before the first request. Keep
        // this as a local timer spin because reset is currently handled in
        // the controller's polling/IRQ path and cannot sleep here.
        let ticks = CPU_CIRCLE / 100;
        let start = get_time_tick();
        while get_time_tick().wrapping_sub(start) < ticks {
            core::hint::spin_loop();
        }
    }

    fn remove_device(&self, port_id: u8) {
        crate::fs::sys::dev::remove_usb_device(self.bus_number, port_id);
        self.devices.lock(|devices| {
            devices.retain(|device| device.lock(|device| device.port_id != port_id));
        });
    }
}

unsafe fn write_context_u32(memory: &DmaMemory, offset: usize, value: u32) {
    write_volatile(memory.as_ptr::<u32>(offset), value);
}

fn endpoint_type_code(endpoint: EndpointDescriptor) -> u32 {
    match (endpoint.transfer_type, endpoint.address.direction) {
        (EndpointTransferType::Control, _) => 4,
        (EndpointTransferType::Isochronous, EndpointDirection::Out) => 1,
        (EndpointTransferType::Isochronous, EndpointDirection::In) => 5,
        (EndpointTransferType::Bulk, EndpointDirection::Out) => 2,
        (EndpointTransferType::Bulk, EndpointDirection::In) => 6,
        (EndpointTransferType::Interrupt, EndpointDirection::Out) => 3,
        (EndpointTransferType::Interrupt, EndpointDirection::In) => 7,
    }
}

/// xHCI encodes periodic USB intervals as powers of two microframes.
fn xhci_interval(speed: PortSpeed, endpoint: EndpointDescriptor) -> u8 {
    match (speed, endpoint.transfer_type) {
        (PortSpeed::FullSpeed | PortSpeed::LowSpeed, EndpointTransferType::Interrupt) => {
            // FS/LS interrupt bInterval is expressed in frames. xHCI asks
            // for the largest power-of-two microframe period not exceeding
            // bInterval * 8 (xHCI 6.2.3.6, Table 6-12).
            let microframes = usize::from(endpoint.interval.max(1)) * 8;
            let mut period: usize = 1;
            let mut encoded = 0;
            while period.saturating_mul(2) <= microframes {
                period *= 2;
                encoded += 1;
            }
            encoded.clamp(3, 10) as u8
        }
        (PortSpeed::FullSpeed, EndpointTransferType::Isochronous) => {
            // FS isochronous bInterval is already an exponent in frames.
            endpoint.interval.saturating_add(2).clamp(3, 18)
        }
        (PortSpeed::HighSpeed | PortSpeed::SuperSpeed, transfer_type)
            if matches!(
                transfer_type,
                EndpointTransferType::Isochronous | EndpointTransferType::Interrupt
            ) =>
        {
            endpoint.interval.saturating_sub(1).min(15)
        }
        _ => 0,
    }
}

/// Decode Device Descriptor bMaxPacketSize0 for the negotiated USB speed.
fn descriptor_ep0_max_packet_size(speed: PortSpeed, encoded: u8) -> Option<u16> {
    match speed {
        // USB 3.x defines this field as log2(MPS) and EP0 is 512 bytes.
        PortSpeed::SuperSpeed => (encoded == 9).then_some(512),
        PortSpeed::FullSpeed | PortSpeed::HighSpeed | PortSpeed::LowSpeed => {
            let size = u16::from(encoded);
            matches!(size, 8 | 16 | 32 | 64).then_some(size)
        }
        PortSpeed::Undefined | PortSpeed::Reserved(_) => None,
    }
}

fn zero_dma(memory: &DmaMemory) {
    unsafe {
        for offset in (0..memory.len()).step_by(core::mem::size_of::<u32>()) {
            write_volatile(memory.as_ptr::<u32>(offset), 0);
        }
    }
}

fn dma_bytes(memory: &DmaMemory, len: usize) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(len);
    for offset in 0..len {
        // SAFETY: callers only request bytes inside their DmaMemory object.
        bytes.push(unsafe { read_volatile(memory.as_ptr::<u8>(offset)) });
    }
    bytes
}

fn portsc_offset(port_id: u8) -> usize {
    PORT_REGISTER_BASE + (port_id as usize - 1) * PORT_REGISTER_STRIDE
}
