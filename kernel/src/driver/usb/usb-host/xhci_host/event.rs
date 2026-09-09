//! xHCI command and event-ring helpers.

use super::{Trb, XhciHost};
use crate::arch::riscv64::driver::dma::dma_write_barrier;
use alloc::sync::Arc;
use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicBool, Ordering};

const TRB_TYPE_ENABLE_SLOT_COMMAND: u32 = 9 << 10;
const TRB_TYPE_DISABLE_SLOT_COMMAND: u32 = 10 << 10;
const TRB_TYPE_ADDRESS_DEVICE_COMMAND: u32 = 11 << 10;
const TRB_TYPE_SETUP_STAGE: u32 = 2 << 10;
const TRB_TYPE_DATA_STAGE: u32 = 3 << 10;
const TRB_TYPE_STATUS_STAGE: u32 = 4 << 10;
const TRB_TYPE_NORMAL: u32 = 1 << 10;
const TRB_TYPE_ISOCH: u32 = 5 << 10;
const TRB_TYPE_TRANSFER_EVENT: u8 = 32;
const TRB_TYPE_COMMAND_COMPLETION_EVENT: u8 = 33;
const TRB_TYPE_PORT_STATUS_CHANGE_EVENT: u8 = 34;
const TRB_TYPE_HOST_CONTROLLER_EVENT: u8 = 37;

const TRB_INTERRUPT_ON_SHORT_PACKET: u32 = 1 << 2;
const TRB_CHAIN: u32 = 1 << 4;
const TRB_INTERRUPT_ON_COMPLETION: u32 = 1 << 5;
const TRB_IMMEDIATE_DATA: u32 = 1 << 6;
const TRB_DIRECTION_IN: u32 = 1 << 16;
const USB_REQUEST_DIRECTION_IN: u8 = 1 << 7;
const TRB_SETUP_TRT_NO_DATA: u32 = 0;
const TRB_SETUP_TRT_OUT: u32 = 2 << 16;
const TRB_SETUP_TRT_IN: u32 = 3 << 16;
const TRB_SETUP_PACKET_LENGTH: u32 = 8;
const TRB_TRANSFER_LENGTH_MASK: u32 = 0x1ffff;
const COMPLETION_CODE_SHIFT: u32 = 24;
const SLOT_ID_SHIFT: u32 = 24;
const ENDPOINT_ID_SHIFT: u32 = 16;

// PORTSC fields from xHCI Table 5-27. Bits 17:23 are RW1CS: writing one
// clears the corresponding latched change indication. PED (bit 1) is also
// RW1CS, but it is a control operation and must not be cleared here by an
// event acknowledgement.
const PORTSC_CCS: u32 = 1 << 0;
const PORTSC_PED: u32 = 1 << 1;
const PORTSC_OCA: u32 = 1 << 3;
const PORTSC_PLS_SHIFT: u32 = 5;
const PORTSC_PLS_MASK: u32 = 0xf << PORTSC_PLS_SHIFT;
const PORTSC_PP: u32 = 1 << 9;
const PORTSC_PIC_MASK: u32 = 0x3 << 14;
const PORTSC_SPEED_SHIFT: u32 = 10;
const PORTSC_SPEED_MASK: u32 = 0xf << PORTSC_SPEED_SHIFT;
const PORTSC_CSC: u32 = 1 << 17;
const PORTSC_PEC: u32 = 1 << 18;
const PORTSC_WRC: u32 = 1 << 19;
const PORTSC_OCC: u32 = 1 << 20;
const PORTSC_PRC: u32 = 1 << 21;
const PORTSC_PLC: u32 = 1 << 22;
const PORTSC_CEC: u32 = 1 << 23;
const PORTSC_PR: u32 = 1 << 4;
const PORTSC_WPR: u32 = 1 << 31;
const PORTSC_CAS: u32 = 1 << 24;
const PORTSC_WCE_WDE_WOE: u32 = 0x7 << 25;
const PORTSC_DEVICE_REMOVABLE: u32 = 1 << 30;
// PORTSC fields that must survive a write. RWS fields clear when written as
// zero, so a write containing only the RW1CS bits would power down or change
// the link state of the port.
const PORTSC_RO_MASK: u32 =
    PORTSC_CCS | PORTSC_OCA | PORTSC_SPEED_MASK | PORTSC_CAS | PORTSC_DEVICE_REMOVABLE;
const PORTSC_RWS_MASK: u32 = PORTSC_PLS_MASK | PORTSC_PP | PORTSC_PIC_MASK | PORTSC_WCE_WDE_WOE;
const PORTSC_CHANGE_RW1CS_MASK: u32 =
    PORTSC_CSC | PORTSC_PEC | PORTSC_WRC | PORTSC_OCC | PORTSC_PRC | PORTSC_PLC | PORTSC_CEC;

/// Build a PORTSC write while preserving all status/control fields that must
/// survive a read-modify-write. `write_bits` may contain only intentional
/// RW1S (PR/WPR) or RW1CS bits.
pub(crate) fn portsc_neutral_value(portsc: u32, write_bits: u32) -> u32 {
    (portsc & (PORTSC_RO_MASK | PORTSC_RWS_MASK))
        | (write_bits & (PORTSC_CHANGE_RW1CS_MASK | PORTSC_PR | PORTSC_WPR))
}

pub(crate) fn portsc_event_ack_value(portsc: u32) -> u32 {
    let change_bits = portsc & PORTSC_CHANGE_RW1CS_MASK;
    if change_bits == 0 {
        return 0;
    }
    portsc_neutral_value(portsc, change_bits)
}

/// USB speed reported by the xHCI PORTSC Port Speed field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortSpeed {
    /// The port has not been initialized or has no connected device.
    Undefined,
    /// USB 1.x full-speed, 12 Mb/s.
    FullSpeed,
    /// USB 1.x low-speed, 1.5 Mb/s.
    LowSpeed,
    /// USB 2.0 high-speed, 480 Mb/s.
    HighSpeed,
    /// USB 3.x SuperSpeed, 5 Gb/s.
    SuperSpeed,
    /// Reserved speed encoding reported by the controller.
    Reserved(u8),
}

impl PortSpeed {
    pub(crate) fn from_portsc(portsc: u32) -> Self {
        match ((portsc & PORTSC_SPEED_MASK) >> PORTSC_SPEED_SHIFT) as u8 {
            0 => Self::Undefined,
            1 => Self::FullSpeed,
            2 => Self::LowSpeed,
            3 => Self::HighSpeed,
            4 => Self::SuperSpeed,
            value => Self::Reserved(value),
        }
    }

    pub(crate) fn context_code(self) -> u32 {
        match self {
            Self::Undefined => 0,
            Self::FullSpeed => 1,
            Self::LowSpeed => 2,
            Self::HighSpeed => 3,
            Self::SuperSpeed => 4,
            Self::Reserved(value) => u32::from(value),
        }
    }

    pub(crate) fn ep0_max_packet_size(self) -> u16 {
        match self {
            Self::LowSpeed => 8,
            // Full-speed EP0 starts with the USB default. Read
            // bMaxPacketSize0 and use Evaluate Context for the final value.
            Self::FullSpeed => 8,
            Self::HighSpeed => 64,
            Self::SuperSpeed => 512,
            Self::Undefined | Self::Reserved(_) => 0,
        }
    }
}

/// Current USB link state reported by the xHCI PORTSC PLS field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortLinkState {
    U0,
    U1,
    U2,
    U3,
    Disabled,
    RxDetect,
    Inactive,
    Polling,
    Recovery,
    HotReset,
    Compliance,
    TestMode,
    Resume,
    Reserved(u8),
}

impl PortLinkState {
    fn from_portsc(portsc: u32) -> Self {
        match ((portsc & PORTSC_PLS_MASK) >> PORTSC_PLS_SHIFT) as u8 {
            0 => Self::U0,
            1 => Self::U1,
            2 => Self::U2,
            3 => Self::U3,
            4 => Self::Disabled,
            5 => Self::RxDetect,
            6 => Self::Inactive,
            7 => Self::Polling,
            8 => Self::Recovery,
            9 => Self::HotReset,
            10 => Self::Compliance,
            11 => Self::TestMode,
            15 => Self::Resume,
            value => Self::Reserved(value),
        }
    }
}

/// The event a task is allowed to wait for.
///
/// The variant identifies the event type and its field identifies one
/// controller transaction. Controller-global events intentionally have no
/// variant here: they are delivered only to the host-device handlers.
#[derive(Clone, Debug)]
pub enum EventWaiter {
    /// Completion of one command TRB.
    CommandCompletion {
        /// Physical address of the command TRB.
        command_trb_pointer: u64,
        /// Set by the IRQ path when the matching event is consumed.
        completed: Arc<AtomicBool>,
    },
    /// Completion of one transfer TRB or Event Data TRB.
    Transfer {
        /// Physical address reported by the Transfer Event TRB.
        trb_pointer: u64,
        /// Set by the IRQ path when the matching event is consumed.
        completed: Arc<AtomicBool>,
    },
}

impl EventWaiter {
    /// Create a waiter for a transfer TRB submitted by software.
    pub(crate) fn transfer(trb_pointer: u64) -> Self {
        Self::Transfer {
            trb_pointer,
            completed: Arc::new(AtomicBool::new(false)),
        }
    }

    pub(crate) fn is_completed(&self) -> bool {
        match self {
            Self::CommandCompletion { completed, .. } | Self::Transfer { completed, .. } => {
                completed.load(Ordering::Acquire)
            }
        }
    }

    pub(crate) fn complete(&self) {
        match self {
            Self::CommandCompletion { completed, .. } | Self::Transfer { completed, .. } => {
                completed.store(true, Ordering::Release)
            }
        }
    }
}

impl PartialEq for EventWaiter {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::CommandCompletion {
                    command_trb_pointer: left_pointer,
                    completed: left_completed,
                },
                Self::CommandCompletion {
                    command_trb_pointer: right_pointer,
                    completed: right_completed,
                },
            ) => left_pointer == right_pointer && Arc::ptr_eq(left_completed, right_completed),
            (
                Self::Transfer {
                    trb_pointer: left_pointer,
                    completed: left_completed,
                },
                Self::Transfer {
                    trb_pointer: right_pointer,
                    completed: right_completed,
                },
            ) => left_pointer == right_pointer && Arc::ptr_eq(left_completed, right_completed),
            _ => false,
        }
    }
}

impl Eq for EventWaiter {}

/// A command that can be submitted to the xHCI command ring.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XhciCommand {
    /// Allocate one device slot and return its ID in the completion event.
    EnableSlot,
    /// Disable a device slot after the device has disconnected.
    DisableSlot { slot_id: u8 },
    /// Initialize the slot and EP0, then let the xHC issue SET_ADDRESS.
    AddressDevice {
        slot_id: u8,
        input_context_pointer: u64,
    },
}

impl XhciCommand {
    fn into_trb(self, cycle: bool) -> Trb {
        let cycle_bit = if cycle { super::TRB_CYCLE } else { 0 };
        match self {
            Self::EnableSlot => Trb {
                parameter: 0,
                status: 0,
                control: TRB_TYPE_ENABLE_SLOT_COMMAND | cycle_bit,
            },
            Self::DisableSlot { slot_id } => Trb {
                parameter: 0,
                status: 0,
                control: TRB_TYPE_DISABLE_SLOT_COMMAND
                    | (u32::from(slot_id) << SLOT_ID_SHIFT)
                    | cycle_bit,
            },
            Self::AddressDevice {
                slot_id,
                input_context_pointer,
            } => Trb {
                parameter: input_context_pointer,
                status: 0,
                // BSR is deliberately zero: the xHC must issue SET_ADDRESS.
                control: TRB_TYPE_ADDRESS_DEVICE_COMMAND
                    | (u32::from(slot_id) << SLOT_ID_SHIFT)
                    | cycle_bit,
            },
        }
    }
}

/// USB 2.0/3.x standard 8-byte control-transfer setup packet.
///
/// The fields are kept in USB wire order. xHCI carries the complete packet
/// as immediate data in the Setup Stage TRB parameter field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UsbSetup {
    /// Request direction, type and recipient (USB bmRequestType).
    pub bm_request_type: u8,
    /// Standard, class or vendor request (USB bRequest).
    pub b_request: u8,
    /// Request-specific value (USB wValue, little endian on the bus).
    pub w_value: u16,
    /// Request-specific index (USB wIndex, little endian on the bus).
    pub w_index: u16,
    /// Number of bytes in the optional Data Stage (USB wLength).
    pub w_length: u16,
}

impl UsbSetup {
    /// Encode this setup packet as an xHCI Setup Stage TRB.
    pub fn to_trb(self, cycle: bool) -> Trb {
        let setup_data = u64::from_le_bytes([
            self.bm_request_type,
            self.b_request,
            self.w_value as u8,
            (self.w_value >> 8) as u8,
            self.w_index as u8,
            (self.w_index >> 8) as u8,
            self.w_length as u8,
            (self.w_length >> 8) as u8,
        ]);
        let transfer_type = if self.w_length == 0 {
            TRB_SETUP_TRT_NO_DATA
        } else if self.bm_request_type & USB_REQUEST_DIRECTION_IN != 0 {
            TRB_SETUP_TRT_IN
        } else {
            TRB_SETUP_TRT_OUT
        };
        let cycle_bit = if cycle { super::TRB_CYCLE } else { 0 };

        Trb {
            parameter: setup_data,
            // The Setup Stage always transfers exactly the eight setup bytes.
            status: TRB_SETUP_PACKET_LENGTH,
            // Setup is followed by Data (or directly by Status), so it is
            // chained. IDT is required because the setup packet is inline.
            control: TRB_TYPE_SETUP_STAGE
                | transfer_type
                | TRB_CHAIN
                | TRB_IMMEDIATE_DATA
                | cycle_bit,
        }
    }

    /// Whether the optional Data Stage travels from the device to the host.
    pub fn data_direction_in(self) -> bool {
        self.bm_request_type & USB_REQUEST_DIRECTION_IN != 0
    }

    /// Build the required Status Stage direction for this setup packet.
    ///
    /// A control transfer has an IN status stage for a request without a
    /// Data Stage. Otherwise the status direction is opposite to Data Stage.
    pub fn status_stage(self) -> UsbStatusStage {
        UsbStatusStage {
            direction_in: self.w_length == 0 || !self.data_direction_in(),
        }
    }
}

/// xHCI Data Stage TRB for a control transfer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UsbDataStage {
    /// Device-visible data buffer address.
    pub buffer: u64,
    /// Number of bytes to transfer.
    pub length: u32,
    /// True for device-to-host (IN), false for host-to-device (OUT).
    pub direction_in: bool,
    /// Request an event when a short packet terminates this stage.
    pub interrupt_on_short_packet: bool,
}

impl UsbDataStage {
    /// Encode a chained Data Stage TRB. The following Status Stage completes
    /// the control transfer, so IOC belongs on Status, not Data.
    pub fn to_trb(self, cycle: bool) -> Trb {
        let cycle_bit = if cycle { super::TRB_CYCLE } else { 0 };
        let direction_bit = if self.direction_in {
            TRB_DIRECTION_IN
        } else {
            0
        };
        let short_packet_bit = if self.interrupt_on_short_packet {
            TRB_INTERRUPT_ON_SHORT_PACKET
        } else {
            0
        };

        Trb {
            parameter: self.buffer,
            status: self.length & TRB_TRANSFER_LENGTH_MASK,
            control: TRB_TYPE_DATA_STAGE | direction_bit | short_packet_bit | TRB_CHAIN | cycle_bit,
        }
    }
}

/// xHCI Status Stage TRB for a control transfer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UsbStatusStage {
    /// True when the status handshake is an IN transaction.
    pub direction_in: bool,
}

impl UsbStatusStage {
    /// Encode the final, unchained Status Stage TRB.
    pub fn to_trb(self, cycle: bool) -> Trb {
        let cycle_bit = if cycle { super::TRB_CYCLE } else { 0 };
        let direction_bit = if self.direction_in {
            TRB_DIRECTION_IN
        } else {
            0
        };

        Trb {
            parameter: 0,
            status: 0,
            // Status is the final stage, therefore it owns IOC.
            control: TRB_TYPE_STATUS_STAGE
                | direction_bit
                | TRB_INTERRUPT_ON_COMPLETION
                | cycle_bit,
        }
    }
}

/// A data-transfer TRB format with the required interrupt policy.
///
/// Every transfer TRB generated by this type has IOC set.  ISP is set only
/// when the caller asks to receive an interrupt for a short packet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferTrb {
    /// Normal TRB for bulk, interrupt, and control-data transfers.
    Normal {
        /// Device-visible buffer address.
        buffer: u64,
        /// Number of bytes in the buffer.
        length: u32,
        /// Interrupt when the controller completes a short packet.
        interrupt_on_short_packet: bool,
    },
    /// Isochronous TRB for periodic time-sensitive transfers.
    Isoch {
        /// Device-visible buffer address.
        buffer: u64,
        /// Number of bytes in the buffer.
        length: u32,
        /// Interrupt when the controller completes a short packet.
        interrupt_on_short_packet: bool,
    },
}

impl TransferTrb {
    /// Encode the transfer TRB with the supplied producer cycle state.
    pub fn to_trb(self, cycle: bool) -> Trb {
        let (trb_type, buffer, length, interrupt_on_short_packet) = match self {
            Self::Normal {
                buffer,
                length,
                interrupt_on_short_packet,
            } => (TRB_TYPE_NORMAL, buffer, length, interrupt_on_short_packet),
            Self::Isoch {
                buffer,
                length,
                interrupt_on_short_packet,
            } => (TRB_TYPE_ISOCH, buffer, length, interrupt_on_short_packet),
        };

        let cycle_bit = if cycle { super::TRB_CYCLE } else { 0 };
        let short_packet_bit = if interrupt_on_short_packet {
            TRB_INTERRUPT_ON_SHORT_PACKET
        } else {
            0
        };
        Trb {
            parameter: buffer,
            // Transfer Length occupies bits 16:0 of the status field.
            status: length & 0x1ffff,
            control: trb_type | short_packet_bit | TRB_INTERRUPT_ON_COMPLETION | cycle_bit,
        }
    }
}

/// xHCI completion code carried by an event TRB.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompletionCode {
    /// The command or transfer completed successfully.
    Success,
    /// A completion code not modeled by this minimal driver.
    Other(u8),
}

impl CompletionCode {
    fn from_status(status: u32) -> Self {
        match (status >> COMPLETION_CODE_SHIFT) as u8 {
            1 => Self::Success,
            code => Self::Other(code),
        }
    }
}

/// Decoded xHCI event types.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XhciEvent {
    /// Completion of a command submitted to the command ring.
    CommandCompletion(CommandCompletionEvent),
    /// Completion of a transfer on a device endpoint.
    Transfer(TransferEvent),
    /// A root-port status change notification.
    PortStatusChange(PortStatusChangeEvent),
    /// A host-controller error event.
    HostController(HostControllerEvent),
}

/// Fields decoded from a Command Completion Event TRB.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandCompletionEvent {
    /// Physical address of the completed command TRB.
    pub command_trb_pointer: u64,
    /// Command completion result.
    pub completion_code: CompletionCode,
    /// Device slot associated with the command, if any.
    pub slot_id: u8,
}

/// Fields decoded from a Transfer Event TRB.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransferEvent {
    /// Physical address of the transfer TRB or Event Data TRB.
    pub trb_pointer: u64,
    /// Transfer completion result.
    pub completion_code: CompletionCode,
    /// Device slot associated with the transfer.
    pub slot_id: u8,
    /// Endpoint associated with the transfer.
    pub endpoint_id: u8,
}

/// Fields decoded from a Port Status Change Event TRB.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PortStatusChangeEvent {
    /// Root-port number reported by the controller.
    pub port_id: u8,
    /// Raw PORTSC value sampled while handling the event.
    pub portsc: u32,
    /// Current Connect Status: a device is physically connected when true.
    pub ccs: bool,
    /// Connect Status Change latch: connection state changed since last clear.
    pub csc: bool,
    /// Port Enabled/Disabled status.
    pub ped: bool,
    /// Over-current active status.
    pub over_current: bool,
    /// Current USB link power state.
    pub link_state: PortLinkState,
    /// Port power status.
    pub powered: bool,
    /// USB device speed reported by PORTSC.
    pub speed: PortSpeed,
    /// Port enable changed.
    pub pec: bool,
    /// Warm reset completed.
    pub wrc: bool,
    /// Over-current condition changed.
    pub occ: bool,
    /// Port reset completed.
    pub prc: bool,
    /// Port link state changed.
    pub plc: bool,
    /// Port configure error changed.
    pub cec: bool,
    /// Cold attach status. This is cleared by a warm reset, not by RW1C.
    pub cas: bool,
}

impl PortStatusChangeEvent {
    pub(crate) fn from_portsc(port_id: u8, portsc: u32) -> Self {
        Self {
            port_id,
            portsc,
            ccs: portsc & PORTSC_CCS != 0,
            csc: portsc & PORTSC_CSC != 0,
            ped: portsc & PORTSC_PED != 0,
            over_current: portsc & PORTSC_OCA != 0,
            link_state: PortLinkState::from_portsc(portsc),
            powered: portsc & PORTSC_PP != 0,
            speed: PortSpeed::from_portsc(portsc),
            pec: portsc & PORTSC_PEC != 0,
            wrc: portsc & PORTSC_WRC != 0,
            occ: portsc & PORTSC_OCC != 0,
            prc: portsc & PORTSC_PRC != 0,
            plc: portsc & PORTSC_PLC != 0,
            cec: portsc & PORTSC_CEC != 0,
            cas: portsc & PORTSC_CAS != 0,
        }
    }

    /// Return whether PORTSC still has a latched change for this event.
    ///
    /// The event TRB may already be in the ring while software handles and
    /// acknowledges the same change. A later IRQ can therefore observe a
    /// valid event TRB with no current PORTSC change bits; that stale event
    /// must not be reported as a new insertion/removal.
    pub(crate) fn has_change(&self) -> bool {
        self.csc || self.pec || self.wrc || self.occ || self.prc || self.plc || self.cec
    }
}

/// Fields decoded from a Host Controller Event TRB.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostControllerEvent {
    /// Host-controller event completion code.
    pub completion_code: CompletionCode,
}

impl XhciEvent {
    /// Decode the type-specific fields of one raw event TRB.
    pub fn from_trb(trb: Trb) -> Result<Self, &'static str> {
        let event = match trb.trb_type() {
            TRB_TYPE_COMMAND_COMPLETION_EVENT => Self::CommandCompletion(CommandCompletionEvent {
                command_trb_pointer: trb.parameter,
                completion_code: CompletionCode::from_status(trb.status),
                slot_id: (trb.control >> SLOT_ID_SHIFT) as u8,
            }),
            TRB_TYPE_TRANSFER_EVENT => Self::Transfer(TransferEvent {
                trb_pointer: trb.parameter,
                completion_code: CompletionCode::from_status(trb.status),
                slot_id: (trb.control >> SLOT_ID_SHIFT) as u8,
                endpoint_id: (trb.control >> ENDPOINT_ID_SHIFT) as u8 & 0x1f,
            }),
            TRB_TYPE_PORT_STATUS_CHANGE_EVENT => Self::PortStatusChange(
                PortStatusChangeEvent::from_portsc((trb.parameter >> 24) as u8, 0),
            ),
            TRB_TYPE_HOST_CONTROLLER_EVENT => Self::HostController(HostControllerEvent {
                completion_code: CompletionCode::from_status(trb.status),
            }),
            _ => return Err("xHCI unknown event TRB type"),
        };
        Ok(event)
    }
}

impl EventWaiter {
    /// Match an already registered waiter against a decoded event.
    ///
    /// A waiter is created by the command/transfer submission path. Event
    /// decoding only checks that existing identity; it never creates one.
    pub(crate) fn matches(&self, event: &XhciEvent) -> bool {
        match (self, event) {
            (
                Self::CommandCompletion {
                    command_trb_pointer,
                    ..
                },
                XhciEvent::CommandCompletion(event),
            ) => *command_trb_pointer == event.command_trb_pointer,
            (Self::Transfer { trb_pointer, .. }, XhciEvent::Transfer(event)) => {
                *trb_pointer == event.trb_pointer
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        portsc_event_ack_value, portsc_neutral_value, TransferTrb, UsbDataStage, UsbSetup,
        XhciCommand, PORTSC_CHANGE_RW1CS_MASK, PORTSC_PIC_MASK, PORTSC_PLS_MASK, PORTSC_PP,
        PORTSC_PR, PORTSC_RWS_MASK, TRB_CHAIN, TRB_DIRECTION_IN, TRB_IMMEDIATE_DATA,
        TRB_INTERRUPT_ON_COMPLETION, TRB_INTERRUPT_ON_SHORT_PACKET, TRB_SETUP_TRT_IN,
        TRB_TYPE_DATA_STAGE, TRB_TYPE_SETUP_STAGE, TRB_TYPE_STATUS_STAGE,
    };

    #[test]
    fn control_transfer_stages_encode_xhci_fields() {
        let setup = UsbSetup {
            bm_request_type: 0x80,
            b_request: 6,
            w_value: 0x0100,
            w_index: 0,
            w_length: 18,
        };
        let setup_trb = setup.to_trb(true);
        assert_eq!(setup_trb.trb_type(), 2);
        assert_eq!(setup_trb.status, 8);
        assert_eq!(setup_trb.parameter, 0x0012_0000_0100_0680);
        assert_ne!(setup_trb.control & TRB_SETUP_TRT_IN, 0);
        assert_ne!(setup_trb.control & (TRB_CHAIN | TRB_IMMEDIATE_DATA), 0);

        let data_trb = UsbDataStage {
            buffer: 0x4000,
            length: 18,
            direction_in: true,
            interrupt_on_short_packet: true,
        }
        .to_trb(true);
        assert_eq!(data_trb.trb_type(), 3);
        assert_eq!(data_trb.parameter, 0x4000);
        assert_ne!(data_trb.control & TRB_DIRECTION_IN, 0);
        assert_ne!(data_trb.control & TRB_CHAIN, 0);
        assert_ne!(data_trb.control & TRB_INTERRUPT_ON_SHORT_PACKET, 0);
        assert_eq!(data_trb.control & TRB_INTERRUPT_ON_COMPLETION, 0);

        let status_trb = setup.status_stage().to_trb(true);
        assert_eq!(status_trb.trb_type(), 4);
        assert_eq!(status_trb.control & TRB_TYPE_SETUP_STAGE, 0);
        assert_eq!(status_trb.control & TRB_TYPE_DATA_STAGE, 0);
        assert_ne!(status_trb.control & TRB_TYPE_STATUS_STAGE, 0);
        assert_eq!(status_trb.control & TRB_DIRECTION_IN, 0);
        assert_ne!(status_trb.control & TRB_INTERRUPT_ON_COMPLETION, 0);
        assert_eq!(status_trb.control & TRB_CHAIN, 0);
    }

    #[test]
    fn transfer_trbs_request_completion_interrupts() {
        let normal = TransferTrb::Normal {
            buffer: 0x1000,
            length: 512,
            interrupt_on_short_packet: true,
        }
        .to_trb(true);
        assert_ne!(normal.control & TRB_INTERRUPT_ON_COMPLETION, 0);
        assert_ne!(normal.control & TRB_INTERRUPT_ON_SHORT_PACKET, 0);

        let isoch = TransferTrb::Isoch {
            buffer: 0x2000,
            length: 128,
            interrupt_on_short_packet: false,
        }
        .to_trb(true);
        assert_ne!(isoch.control & TRB_INTERRUPT_ON_COMPLETION, 0);
        assert_eq!(isoch.control & TRB_INTERRUPT_ON_SHORT_PACKET, 0);
    }

    #[test]
    fn device_commands_encode_required_fields() {
        let enable = XhciCommand::EnableSlot.into_trb(true);
        assert_eq!(enable.trb_type(), 9);
        assert!(enable.cycle());
        assert_eq!(enable.parameter, 0);

        let address = XhciCommand::AddressDevice {
            slot_id: 3,
            input_context_pointer: 0x1234_0000,
        }
        .into_trb(true);
        assert_eq!(address.trb_type(), 11);
        assert_eq!(address.parameter, 0x1234_0000);
        assert_eq!((address.control >> SLOT_ID_SHIFT) as u8, 3);
        assert_eq!(address.control & (1 << 9), 0); // BSR=0: issue SET_ADDRESS.
    }

    #[test]
    fn portsc_ack_preserves_running_state() {
        let portsc = (5 << 5) // PLS = RxDetect
            | PORTSC_PP
            | (2 << 14) // PIC
            | (1 << 25) // WCE
            | (1 << 17); // CSC
        let write_value = portsc_event_ack_value(portsc);

        assert_eq!(
            write_value & (PORTSC_PLS_MASK | PORTSC_PP | PORTSC_PIC_MASK),
            portsc & (PORTSC_PLS_MASK | PORTSC_PP | PORTSC_PIC_MASK)
        );
        assert_eq!(
            write_value & PORTSC_CHANGE_RW1CS_MASK,
            PORTSC_CHANGE_RW1CS_MASK & portsc
        );
        assert_eq!(
            write_value & !(PORTSC_RWS_MASK | PORTSC_CHANGE_RW1CS_MASK),
            0
        );
    }

    #[test]
    fn portsc_reset_is_neutral_and_keeps_change_latches() {
        let portsc = PORTSC_PP | (1 << 17) | (2 << 14);
        let write_value = portsc_neutral_value(portsc, PORTSC_PR);

        assert_ne!(write_value & PORTSC_PR, 0);
        assert_ne!(write_value & PORTSC_PP, 0);
        assert_eq!(write_value & PORTSC_CHANGE_RW1CS_MASK, 0);
    }
}

impl XhciHost {
    /// Put one command TRB on the command ring and return its waiter.
    pub fn send_command(&mut self, command: XhciCommand) -> EventWaiter {
        // The last TRB is permanently reserved for the Link TRB.
        if self.command_enqueue == super::TRBS_PER_RING - 1 {
            self.command_enqueue = 0;
            self.command_cycle = !self.command_cycle;
        }

        let index = self.command_enqueue;
        let command_pointer = self.command_ring.phys_addr() + index * super::TRB_SIZE;
        let waiter = EventWaiter::CommandCompletion {
            command_trb_pointer: command_pointer as u64,
            completed: Arc::new(AtomicBool::new(false)),
        };

        // Register before writing/ringing the command.  The controller may
        // complete a No Op very quickly, so registering in wait_event_loop
        // would leave a race where the IRQ discards the event first.
        self.register_waiter(waiter.clone());
        unsafe {
            write_volatile(
                self.command_ring.as_ptr::<Trb>(index * super::TRB_SIZE),
                command.into_trb(self.command_cycle),
            );
        }
        self.command_enqueue += 1;
        self.command_ring.clean_for_device();
        waiter
    }

    /// Ring the host-controller doorbell (DB Target 0).
    pub fn bite_dorbell(&self) {
        // The controller fetches the command ring after this MMIO write.
        // Clean it here as well as in send_command so the API is safe when a
        // caller edits/submits a command and then rings the doorbell.
        self.command_ring.clean_for_device();
        dma_write_barrier();
        self.platform.write32(self.doorbell_offset, 0);
    }

    /// Wait for one identified event using the IRQ-updated waiter flag.
    pub fn wait_event_irq(&mut self, waiter: EventWaiter) -> Result<(), &'static str> {
        self.wait_event_loop(waiter)
    }

    /// Wait for one event by polling instead of blocking the current task.
    pub fn wait_event_loop(&mut self, waiter: EventWaiter) -> Result<(), &'static str> {
        self.register_waiter(waiter.clone());
        let result = self.wait_event_loop_inner(&waiter);
        self.unregister_waiter(&waiter);
        result
    }

    fn wait_event_loop_inner(&mut self, waiter: &EventWaiter) -> Result<(), &'static str> {
        // Only the IRQ path reads and consumes the Event Ring. This loop
        // observes the completion flag published by that IRQ path.
        loop {
            if waiter.is_completed() {
                return Ok(());
            }
            core::hint::spin_loop();
        }
    }

    /// Process events raised by this host's interrupter.
    pub(crate) fn register_waiter(&self, waiter: EventWaiter) {
        self.registered_waiters.lock(|waiters| {
            if !waiters.contains(&waiter) {
                waiters.push(waiter);
            }
        });
    }

    pub(crate) fn unregister_waiter(&self, waiter: &EventWaiter) {
        self.registered_waiters.lock(|waiters| {
            if let Some(index) = waiters.iter().position(|registered| registered == waiter) {
                waiters.remove(index);
            }
        });
    }

    pub(crate) fn peek_event(&self) -> Option<Trb> {
        self.event_ring.invalidate_for_cpu();
        let trb = unsafe {
            read_volatile(
                self.event_ring
                    .as_ptr::<Trb>(self.event_dequeue * super::TRB_SIZE),
            )
        };
        (trb.cycle() == self.event_cycle).then_some(trb)
    }

    pub(crate) fn take_next_event(&mut self) -> Option<Trb> {
        let trb = self.peek_event()?;
        self.advance_event_ring();
        Some(trb)
    }

    pub(crate) fn advance_event_ring(&mut self) {
        self.event_dequeue += 1;
        if self.event_dequeue == super::TRBS_PER_RING {
            self.event_dequeue = 0;
            self.event_cycle = !self.event_cycle;
        }

        let dequeue_pointer = self.event_ring.phys_addr() + self.event_dequeue * super::TRB_SIZE;
        dma_write_barrier();
        // ERDP.EHB is write-one-to-clear.  The pointer alone does not clear
        // Event Handler Busy, so the controller would keep the interrupter
        // busy after the event was consumed.
        self.rt_write64(
            super::ERDP,
            dequeue_pointer as u64 | super::ERDP_EVENT_HANDLER_BUSY,
        );
        // IMAN.IP is write-one-to-clear. Preserve IE while acknowledging IP;
        // writing only IP would clear IE and disable later interrupts.
        let iman = self.rt_read32(super::IMAN);
        // Preserve IE and the reserved/read-preserve fields. Only IP is
        // deliberately forced to one, which is the RW1C acknowledgement.
        self.rt_write32(super::IMAN, iman | super::IMAN_INTERRUPT_PENDING);
        // A read-back makes the posted INTx acknowledgement visible before
        // returning from the IRQ path.
        let _ = self.rt_read32(super::IMAN);
    }
}
