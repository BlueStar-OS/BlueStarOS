//! Common xHCI root-port reset and device bookkeeping.

use super::event::{
    portsc_neutral_value, CommandCompletionEvent, CompletionCode, EventWaiter, PortLinkState,
    PortSpeed, PortStatusChangeEvent, TransferEvent, UsbDataStage, UsbSetup, UsbStatusStage,
    XhciCommand,
};
use super::{
    Trb, XhciHost, DMA_PAGE_SIZE, PORT_REGISTER_BASE, PORT_REGISTER_STRIDE, TRBS_PER_RING,
    TRB_CYCLE, TRB_LINK_TOGGLE_CYCLE, TRB_SIZE, TRB_TYPE_LINK,
};
use crate::arch::riscv64::driver::dma::{dma_write_barrier, DmaMemory};
use crate::config::CPU_CIRCLE;
use crate::time::get_time_tick;
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
const INPUT_CONTROL_CONTEXT_ADD_CONTEXT_OFFSET: usize = 0x04;
const INPUT_CONTROL_CONTEXT_ADD_SLOT: u32 = 1 << 0;
const INPUT_CONTROL_CONTEXT_ADD_EP0: u32 = 1 << 1;

// Slot Context, xHCI 6.2.2.1.
const SLOT_CONTEXT_DWORD0_OFFSET: usize = 0x00;
const SLOT_CONTEXT_DWORD1_OFFSET: usize = 0x04;
const SLOT_CONTEXT_SPEED_SHIFT: u32 = 20;
const SLOT_CONTEXT_CONTEXT_ENTRIES: u32 = 1 << 27;
const SLOT_CONTEXT_ROOT_PORT_SHIFT: u32 = 16;

// Endpoint Context, xHCI 6.2.3.1. Endpoint 0 is context index 1.
const ENDPOINT_CONTEXT_DWORD0_OFFSET: usize = 0x00;
const ENDPOINT_CONTEXT_DWORD1_OFFSET: usize = 0x04;
const ENDPOINT_CONTEXT_TR_DEQUEUE_LO_OFFSET: usize = 0x08;
const ENDPOINT_CONTEXT_TR_DEQUEUE_HI_OFFSET: usize = 0x0c;
const ENDPOINT_CONTEXT_CERR: u32 = 3 << 1;
const ENDPOINT_CONTEXT_EP_TYPE_CONTROL: u32 = 4 << 3;
const ENDPOINT_CONTEXT_MAX_PACKET_SIZE_SHIFT: u32 = 16;
const ENDPOINT_CONTEXT_DCS: u32 = 1 << 0;

// xHCI doorbell array, xHCI 5.6.1.
const DOORBELL_REGISTER_STRIDE: usize = 0x04;
const DEVICE_DOORBELL_TARGET_MASK: u8 = 0xff;
const MAX_ENDPOINT_NUMBER: u8 = 15;
const MAX_DEVICE_CONTEXT_INDEX: u8 = 31;

// USB standard Get Descriptor request, USB 2.0 Tables 9-4 and 9-5.
const USB_REQUEST_GET_DESCRIPTOR: u8 = 6;
const USB_DESCRIPTOR_DEVICE: u16 = 1;
const USB_DEVICE_DESCRIPTOR_LENGTH: usize = 18;
const USB_DEVICE_DESCRIPTOR_TYPE_OFFSET: usize = 1;
const USB_DEVICE_DESCRIPTOR_VENDOR_ID_OFFSET: usize = 8;
const USB_DEVICE_DESCRIPTOR_PRODUCT_ID_OFFSET: usize = 10;
const USB_DEVICE_DESCRIPTOR_MIN_LENGTH: usize = 12;
const EP0_DCI: u8 = 1;

/// Events that can advance one device's enumeration state machine.
pub(crate) enum XhciDeviceEvent {
    Command(CommandCompletionEvent),
    Transfer(TransferEvent),
}

/// Device-side enumeration state. The command completion IRQ advances this
/// state; the host does not need a second enumeration state machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum XhciDeviceState {
    /// Enable Slot has been submitted; its completion will assign slot_id.
    EnableSlotPending,
    /// Enable Slot completed; Address Device completion is expected next.
    EnableSlotComplete,
    /// Address Device completed; the first control transfer is in progress.
    GetDescriptorPending,
    /// The device disconnected; Disable Slot is in progress.
    DisableSlotPending,
    /// Address Device completed and the USB address is valid.
    AddressDeviceComplete,
    /// The slot and all device-owned DMA resources have been released.
    Disabled,
}

/// A control transfer currently supported by the minimal USB device layer.
pub enum Transfer {
    /// Read the first device descriptor into the supplied DMA buffer.
    GetDescriptor { buffer: DmaMemory },
}

/// DMA objects that must stay alive while the xHC owns a device slot.
pub struct XhciDeviceContext {
    /// Input Context consumed by Address Device and later commands.
    pub input_context: DmaMemory,
    /// Output Device Context referenced by DCBAA[slot_id].
    pub output_context: DmaMemory,
    /// Default Control Endpoint transfer ring.
    pub ep0_ring: DmaMemory,
}

/// Minimal USB device record kept by the protocol layer.
///
/// Address Device is completed before descriptors are read, therefore VID and
/// PID remain optional while slot ID and device address become available.
pub struct XhciDevice {
    /// USB vendor ID, once the device descriptor has been read.
    pub vendor_id: Option<u16>,
    /// USB product ID, once the device descriptor has been read.
    pub product_id: Option<u16>,
    /// One-based xHCI root-port number.
    pub port_id: u8,
    /// xHCI device slot assigned by Enable Slot.
    pub slot_id: Option<u8>,
    /// USB address selected by the xHC after Address Device.
    pub device_address: Option<u8>,
    /// Speed latched by PORTSC after reset.
    pub speed: PortSpeed,
    /// Input/output contexts and EP0 ring owned by this device.
    pub contexts: Option<XhciDeviceContext>,
    /// Context layout selected from HCCPARAMS1.CSZ for this controller.
    context_stride: usize,
    /// Current enumeration state.
    pub(crate) state: XhciDeviceState,
    /// Producer position and cycle state in the EP0 transfer ring.
    ep0_enqueue: usize,
    ep0_cycle: bool,
    /// Transfer whose DMA buffer is consumed when its Transfer Event arrives.
    pending_transfer: Option<Transfer>,
}

impl XhciDevice {
    pub(crate) fn new(port_id: u8, speed: PortSpeed, context_stride: usize) -> Self {
        Self {
            vendor_id: None,
            product_id: None,
            port_id,
            slot_id: None,
            device_address: None,
            speed,
            contexts: None,
            context_stride,
            state: XhciDeviceState::EnableSlotPending,
            ep0_enqueue: 0,
            ep0_cycle: true,
            pending_transfer: None,
        }
    }

    pub(crate) fn is_enable_slot_pending(&self) -> bool {
        self.state == XhciDeviceState::EnableSlotPending
    }

    pub(crate) fn is_enumerating(&self) -> bool {
        matches!(
            self.state,
            XhciDeviceState::EnableSlotPending
                | XhciDeviceState::EnableSlotComplete
                | XhciDeviceState::GetDescriptorPending
                | XhciDeviceState::DisableSlotPending
        )
    }

    pub(crate) fn is_disabled(&self) -> bool {
        self.state == XhciDeviceState::Disabled
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

    /// Ring this device's doorbell for one endpoint context.
    ///
    /// The doorbell register is selected by the device slot; its value is
    /// the DCI (the endpoint target). The caller must clean the corresponding
    /// transfer ring before calling this function.
    pub(crate) fn device_doorbell(&self, host: &XhciHost, dci: u8) -> Result<(), &'static str> {
        let slot_id = self.slot_id.ok_or("xHCI device has no slot")?;
        if slot_id == 0 || slot_id > host.max_slots {
            return Err("xHCI device slot is invalid");
        }
        if dci == 0 || dci > MAX_DEVICE_CONTEXT_INDEX {
            return Err("xHCI device context index is invalid");
        }

        dma_write_barrier();
        host.platform.write32(
            host.doorbell_offset + usize::from(slot_id) * DOORBELL_REGISTER_STRIDE,
            u32::from(dci & DEVICE_DOORBELL_TARGET_MASK),
        );
        Ok(())
    }

    /// Translate one device-related event into the next device state.
    ///
    /// The device owns context creation, control-transfer submission, and
    /// descriptor parsing. The returned flag tells the host to ring the
    /// command doorbell after this device has been put back into its list.
    pub(crate) fn translate_state(
        &mut self,
        host: &mut XhciHost,
        event: XhciDeviceEvent,
    ) -> Result<bool, &'static str> {
        match (self.state, event) {
            (XhciDeviceState::EnableSlotPending, XhciDeviceEvent::Command(command)) => {
                if command.completion_code != CompletionCode::Success {
                    return Err("xHCI Enable Slot did not complete successfully");
                }
                if command.slot_id == 0 || command.slot_id > host.max_slots {
                    return Err("xHCI Enable Slot returned an invalid slot");
                }
                let address_command =
                    self.prepare_address_device_command(command.slot_id, &host.dcbaa)?;
                let _waiter = host.send_command(address_command);
                Ok(true)
            }
            (XhciDeviceState::EnableSlotComplete, XhciDeviceEvent::Command(command)) => {
                if command.completion_code != CompletionCode::Success {
                    return Err("xHCI Address Device did not complete successfully");
                }
                if self.slot_id != Some(command.slot_id) {
                    return Err("xHCI Address Device slot does not match the device");
                }
                let device_address = self.finish_address_device()?;
                info!(
                    "xhci: device addressed: port={}, slot={}, device_address={}",
                    self.port_id,
                    self.slot_id.unwrap_or(0),
                    device_address
                );
                // This is the first request after Address Device. The device
                // owns the DMA buffer until the matching Transfer Event.
                let buffer =
                    DmaMemory::new(DMA_PAGE_SIZE).ok_or("cannot allocate descriptor buffer")?;
                self.control_transfer(host, Transfer::GetDescriptor { buffer })?;
                Ok(false)
            }
            (XhciDeviceState::GetDescriptorPending, XhciDeviceEvent::Transfer(transfer)) => {
                self.finish_get_descriptor(transfer)
            }
            (XhciDeviceState::DisableSlotPending, XhciDeviceEvent::Command(command)) => {
                if command.completion_code != CompletionCode::Success {
                    return Err("xHCI Disable Slot did not complete successfully");
                }
                if self.slot_id != Some(command.slot_id) {
                    return Err("xHCI Disable Slot ID does not match the device");
                }
                self.finish_disable_slot(&host.dcbaa)?;
                Ok(false)
            }
            (XhciDeviceState::AddressDeviceComplete, XhciDeviceEvent::Command(_)) => {
                Err("xHCI command arrived for an already addressed device")
            }
            (_, XhciDeviceEvent::Transfer(_)) => {
                Err("xHCI transfer arrived in an invalid device state")
            }
            (_, XhciDeviceEvent::Command(_)) => {
                Err("xHCI command arrived in an invalid device state")
            }
        }
    }

    /// Begin the slot shutdown required after a disconnect notification.
    pub(crate) fn disable_slot(&mut self) -> Result<XhciCommand, &'static str> {
        let slot_id = self.slot_id.ok_or("xHCI device has no slot to disable")?;
        if self.state == XhciDeviceState::DisableSlotPending {
            return Err("xHCI device Disable Slot is already pending");
        }
        self.state = XhciDeviceState::DisableSlotPending;
        Ok(XhciCommand::DisableSlot { slot_id })
    }

    /// Submit one control transfer on the device's default control endpoint.
    pub(crate) fn control_transfer(
        &mut self,
        host: &XhciHost,
        transfer: Transfer,
    ) -> Result<(), &'static str> {
        if self.state != XhciDeviceState::AddressDeviceComplete {
            return Err("xHCI device is not addressed");
        }

        let (setup, data, status) = match &transfer {
            Transfer::GetDescriptor { buffer } => {
                if buffer.len() < USB_DEVICE_DESCRIPTOR_LENGTH {
                    return Err("descriptor buffer is too small");
                }
                // The controller will write the Data Stage. Discard stale CPU
                // cache lines before handing this buffer to the controller.
                buffer.invalidate_for_cpu();
                let setup = UsbSetup {
                    bm_request_type: 0x80,
                    b_request: USB_REQUEST_GET_DESCRIPTOR,
                    w_value: USB_DESCRIPTOR_DEVICE << 8,
                    w_index: 0,
                    w_length: USB_DEVICE_DESCRIPTOR_LENGTH as u16,
                };
                let data = UsbDataStage {
                    buffer: buffer.phys_addr() as u64,
                    length: USB_DEVICE_DESCRIPTOR_LENGTH as u32,
                    direction_in: true,
                    interrupt_on_short_packet: true,
                };
                (setup, data, setup.status_stage())
            }
        };

        let cycle = self.ep0_cycle;
        let waiter = self.push_ep0transfer_event(
            host,
            [
                setup.to_trb(cycle),
                data.to_trb(cycle),
                status.to_trb(cycle),
            ],
        )?;
        self.pending_transfer = Some(transfer);
        self.state = XhciDeviceState::GetDescriptorPending;

        if let Err(reason) = self.device_doorbell(host, EP0_DCI) {
            host.unregister_waiter(&waiter);
            self.pending_transfer = None;
            self.state = XhciDeviceState::AddressDeviceComplete;
            return Err(reason);
        }
        Ok(())
    }

    /// Append Setup/Data/Status TRBs to the EP0 ring and register the final
    /// Status TRB as the transfer's event identity.
    fn push_ep0transfer_event(
        &mut self,
        host: &XhciHost,
        trbs: [Trb; 3],
    ) -> Result<EventWaiter, &'static str> {
        let enqueue = self.ep0_enqueue;
        if enqueue + trbs.len() > TRBS_PER_RING - 1 {
            return Err("xHCI EP0 transfer ring is full");
        }
        let contexts = self
            .contexts
            .as_ref()
            .ok_or("xHCI device has no EP0 context")?;
        let ring = &contexts.ep0_ring;
        let waiter = EventWaiter::transfer(
            (ring.phys_addr() + (enqueue + trbs.len() - 1) * TRB_SIZE) as u64,
        );
        host.register_waiter(waiter.clone());

        unsafe {
            for (offset, trb) in trbs.into_iter().enumerate() {
                write_volatile(ring.as_ptr::<Trb>((enqueue + offset) * TRB_SIZE), trb);
            }
        }
        ring.clean_for_device();
        self.ep0_enqueue += trbs.len();
        Ok(waiter)
    }

    /// Consume the completed Get Descriptor transfer and fill in VID/PID.
    fn finish_get_descriptor(&mut self, event: TransferEvent) -> Result<bool, &'static str> {
        if event.completion_code != CompletionCode::Success {
            return Err("xHCI Get Descriptor transfer failed");
        }
        if event.slot_id != self.slot_id.unwrap_or(0) || event.endpoint_id != EP0_DCI {
            return Err("xHCI Get Descriptor event does not match EP0");
        }

        let transfer = self
            .pending_transfer
            .take()
            .ok_or("xHCI has no pending control transfer")?;
        let Transfer::GetDescriptor { buffer } = transfer;
        buffer.invalidate_for_cpu();

        let descriptor_length = unsafe { read_volatile(buffer.as_ptr::<u8>(0)) } as usize;
        let descriptor_type =
            unsafe { read_volatile(buffer.as_ptr::<u8>(USB_DEVICE_DESCRIPTOR_TYPE_OFFSET)) };
        if descriptor_length < USB_DEVICE_DESCRIPTOR_MIN_LENGTH
            || descriptor_type != USB_DESCRIPTOR_DEVICE as u8
        {
            return Err("xHCI returned an invalid device descriptor");
        }

        let vendor_id = unsafe { read_le_u16(&buffer, USB_DEVICE_DESCRIPTOR_VENDOR_ID_OFFSET) };
        let product_id = unsafe { read_le_u16(&buffer, USB_DEVICE_DESCRIPTOR_PRODUCT_ID_OFFSET) };
        self.vendor_id = Some(vendor_id);
        self.product_id = Some(product_id);
        self.state = XhciDeviceState::AddressDeviceComplete;
        info!(
            "xhci: device descriptor: port={}, slot={}, address={}, vid={:#06x}, pid={:#06x}",
            self.port_id,
            self.slot_id.unwrap_or(0),
            self.device_address.unwrap_or(0),
            vendor_id,
            product_id,
        );
        Ok(false)
    }

    /// Release the slot-owned context only after Disable Slot completes.
    fn finish_disable_slot(&mut self, dcbaa: &DmaMemory) -> Result<(), &'static str> {
        let slot_id = self.slot_id.take().ok_or("xHCI device has no slot")?;
        unsafe {
            write_volatile(dcbaa.as_ptr::<u64>(usize::from(slot_id) * 8), 0);
        }
        dcbaa.clean_for_device();
        self.pending_transfer = None;
        self.contexts = None;
        self.device_address = None;
        self.state = XhciDeviceState::Disabled;
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
        if !self.is_enable_slot_pending() || slot_id == 0 {
            return Err("xHCI device is not waiting for Enable Slot");
        }

        let input_context = DmaMemory::new(DMA_PAGE_SIZE).ok_or("cannot allocate Input Context")?;
        let output_context =
            DmaMemory::new(DMA_PAGE_SIZE).ok_or("cannot allocate Output Context")?;
        let ep0_ring = DmaMemory::new(DMA_PAGE_SIZE).ok_or("cannot allocate EP0 ring")?;

        let input_slot_context_offset = self.context_stride;
        let input_ep0_context_offset = self.context_stride * 2;
        let ep0_max_packet_size = u32::from(self.speed.ep0_max_packet_size());

        unsafe {
            // Input Control Context: add Slot (A0) and Default Control
            // Endpoint (A1); all Drop Context and other Add Context bits 0.
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
                self.speed.context_code() << SLOT_CONTEXT_SPEED_SHIFT,
            );
            write_context_u32(
                &input_context,
                input_slot_context_offset + SLOT_CONTEXT_DWORD1_OFFSET,
                (u32::from(self.port_id) << SLOT_CONTEXT_ROOT_PORT_SHIFT)
                    | SLOT_CONTEXT_CONTEXT_ENTRIES,
            );

            // Endpoint Context DWORD0: EP State, Mult, streams and LSA stay
            // zero; only Max Packet Size is needed for the initial EP0.
            write_context_u32(
                &input_context,
                input_ep0_context_offset + ENDPOINT_CONTEXT_DWORD0_OFFSET,
                ep0_max_packet_size << ENDPOINT_CONTEXT_MAX_PACKET_SIZE_SHIFT,
            );
            // Endpoint Context DWORD1: CErr=3 and EP Type=Control. Interval,
            // Max Burst and Max ESIT Payload remain zero for EP0.
            write_context_u32(
                &input_context,
                input_ep0_context_offset + ENDPOINT_CONTEXT_DWORD1_OFFSET,
                ENDPOINT_CONTEXT_CERR | ENDPOINT_CONTEXT_EP_TYPE_CONTROL,
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
        self.contexts = Some(XhciDeviceContext {
            input_context,
            output_context,
            ep0_ring,
        });
        self.state = XhciDeviceState::EnableSlotComplete;
        Ok(XhciCommand::AddressDevice {
            slot_id,
            input_context_pointer,
        })
    }

    fn finish_address_device(&mut self) -> Result<u8, &'static str> {
        if self.state != XhciDeviceState::EnableSlotComplete {
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
        self.state = XhciDeviceState::AddressDeviceComplete;
        Ok(device_address)
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
        self.devices.lock(|devices| {
            devices.retain(|device| device.port_id != port_id);
        });
    }
}

unsafe fn write_context_u32(memory: &DmaMemory, offset: usize, value: u32) {
    write_volatile(memory.as_ptr::<u32>(offset), value);
}

unsafe fn read_le_u16(memory: &DmaMemory, offset: usize) -> u16 {
    u16::from_le_bytes([
        read_volatile(memory.as_ptr::<u8>(offset)),
        read_volatile(memory.as_ptr::<u8>(offset + 1)),
    ])
}

fn portsc_offset(port_id: u8) -> usize {
    PORT_REGISTER_BASE + (port_id as usize - 1) * PORT_REGISTER_STRIDE
}
