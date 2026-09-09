//! Common xHCI host-controller protocol support.

use crate::arch::riscv64::driver::dma::{dma_read_barrier, dma_write_barrier, DmaMemory};
use crate::sync::NoIrqLock;
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::ptr::write_volatile;
use lazy_static::lazy_static;
use log::{debug, error, info, warn};

pub mod device;
pub mod event;

const CAPLENGTH: usize = 0x00;
const HCSPARAMS1: usize = 0x04;
const HCCPARAMS1: usize = 0x10;
const DBOFF: usize = 0x14;
const RTSOFF: usize = 0x18;

const USBCMD: usize = 0x00;
const USBSTS: usize = 0x04;
const PAGESIZE: usize = 0x08;
const CRCR: usize = 0x18;
const DCBAAP: usize = 0x30;
const CONFIG: usize = 0x38;

pub(crate) const IMAN: usize = 0x20;
pub(crate) const ERSTSZ: usize = 0x28;
pub(crate) const ERSTBA: usize = 0x30;
pub(crate) const ERDP: usize = 0x38;
pub(crate) const ERDP_EVENT_HANDLER_BUSY: u64 = 1 << 3;
pub(crate) const PORT_REGISTER_BASE: usize = 0x400;
pub(crate) const PORT_REGISTER_STRIDE: usize = 0x10;

const USBCMD_RUN_STOP: u32 = 1 << 0;
const USBCMD_INTERRUPT_ENABLE: u32 = 1 << 2;
const USBCMD_HOST_SYSTEM_ERROR_ENABLE: u32 = 1 << 3;
const USBCMD_EWE: u32 = 1 << 10;
const USBCMD_EU3S: u32 = 1 << 11;
const USBCMD_CME: u32 = 1 << 13;
const USBCMD_ETE: u32 = 1 << 14;
const USBCMD_TSC_ENABLE: u32 = 1 << 15;
const USBCMD_VTIO_ENABLE: u32 = 1 << 16;
// Preserve ordinary RW and RsvdP fields, but never copy command/strobe bits
// such as HCRST, LHCRST, CSS, or CRS from a status read into a write.
const USBCMD_SAFE_WRITE_MASK: u32 = (0x7 << 4)
    | USBCMD_RUN_STOP
    | USBCMD_INTERRUPT_ENABLE
    | USBCMD_HOST_SYSTEM_ERROR_ENABLE
    | (1 << 12)
    | USBCMD_EWE
    | USBCMD_EU3S
    | USBCMD_CME
    | USBCMD_ETE
    | USBCMD_TSC_ENABLE
    | USBCMD_VTIO_ENABLE;
const USBSTS_HCHALTED: u32 = 1 << 0;
// USBSTS fields that are RW1C. Never write back the complete status word:
// HCH, SSS, RSS, CNR, HCE, and reserved bits are not RW1C fields.
pub(crate) const USBSTS_HOST_SYSTEM_ERROR: u32 = 1 << 2;
pub(crate) const USBSTS_EVENT_INTERRUPT: u32 = 1 << 3;
pub(crate) const USBSTS_PORT_CHANGE_DETECT: u32 = 1 << 4;
pub(crate) const USBSTS_SAVE_RESTORE_ERROR: u32 = 1 << 10;
const USBSTS_W1C_MASK: u32 = USBSTS_HOST_SYSTEM_ERROR
    | USBSTS_EVENT_INTERRUPT
    | USBSTS_PORT_CHANGE_DETECT
    | USBSTS_SAVE_RESTORE_ERROR;
const USBSTS_HOST_CONTROLLER_ERROR: u32 = 1 << 12;
const USBSTS_CNR: u32 = 1 << 11;
const HCCPARAMS1_AC64: u32 = 1 << 0;
const HCCPARAMS1_CSZ: u32 = 1 << 2;
const IMAN_INTERRUPT_PENDING: u32 = 1 << 0;
const IMAN_INTERRUPT_ENABLE: u32 = 1 << 1;
pub(crate) const TRB_CYCLE: u32 = 1 << 0;
const TRB_LINK_TOGGLE_CYCLE: u32 = 1 << 1;
const TRB_TYPE_LINK: u32 = 6 << 10;

pub(crate) const TRBS_PER_RING: usize = 256;
pub(crate) const TRB_SIZE: usize = 16;
const DMA_PAGE_SIZE: usize = 4096;
pub(crate) const WAIT_LOOPS: usize = 10_000_000;

/// Platform adapter consumed by the common USB host protocol.
///
/// A platform only needs to provide its identity and three volatile MMIO
/// operations. The xHCI implementation owns this adapter for its lifetime.
pub trait AsUsbHost {
    /// Short platform/controller name used in diagnostics.
    fn name(&self) -> &'static str;

    /// Read one 32-bit register at a BAR-relative offset.
    fn read32(&self, offset: usize) -> u32;

    /// Write one 32-bit register at a BAR-relative offset.
    fn write32(&self, offset: usize, value: u32);

    /// Write one 64-bit register at a BAR-relative offset.
    fn write64(&self, offset: usize, value: u64);
}

lazy_static! {
    /// All initialized xHCI hosts. The registry owns each host's lifetime.
    pub static ref XHCI_HOSTS: NoIrqLock<Vec<Box<XhciHost>>> = NoIrqLock::new(Vec::new());
}

/// Transfer ownership of an initialized host to the xHCI subsystem.
pub fn register_xhci_host(host: XhciHost) {
    let host = Box::new(host);
    info!("xhci: register host {}", host.name());
    XHCI_HOSTS.lock(|hosts| hosts.push(host));
}

/// Dispatch the xHCI PLIC source to every registered host.
fn event_irq_process() {
    XHCI_HOSTS.lock(|hosts| {
        for host in hosts.iter_mut() {
            host.event_irq_process();
        }
    });
}

/// PLIC callback for the QEMU xHCI legacy interrupt route.
pub fn xhci_irq_handler() {
    event_irq_process();
}

/// A raw xHCI Transfer Request Block.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Trb {
    /// TRB parameter or pointer field.
    pub parameter: u64,
    /// TRB status field.
    pub status: u32,
    /// TRB control field.
    pub control: u32,
}

impl Trb {
    /// Return the six-bit TRB type field.
    pub fn trb_type(&self) -> u8 {
        ((self.control >> 10) & 0x3f) as u8
    }

    /// Return the TRB cycle bit.
    pub fn cycle(&self) -> bool {
        self.control & TRB_CYCLE != 0
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ErstEntry {
    segment_base: u64,
    segment_size: u32,
    reserved: u32,
}

/// Common xHCI register and ring initialization.
pub struct XhciHost {
    /// Platform MMIO and identity owned by this protocol host.
    platform: Box<dyn AsUsbHost>,
    operational_offset: usize,
    runtime_offset: usize,
    doorbell_offset: usize,
    max_slots: u8,
    max_ports: u8,
    max_interrupters: u16,
    controller_context_stride: usize,
    command_enqueue: usize,
    command_cycle: bool,
    event_dequeue: usize,
    event_cycle: bool,
    registered_waiters: NoIrqLock<Vec<event::EventWaiter>>,
    pub(crate) devices: NoIrqLock<Vec<device::XhciDevice>>,
    dcbaa: DmaMemory,
    command_ring: DmaMemory,
    event_ring: DmaMemory,
    erst: DmaMemory,
}

impl XhciHost {
    /// Read capabilities, allocate rings, and start one xHCI controller.
    pub fn new(platform: impl AsUsbHost + 'static) -> Result<Self, &'static str> {
        let caplength = (platform.read32(CAPLENGTH) & 0xff) as usize;
        if caplength < 0x20 {
            return Err("xHCI capability length is invalid");
        }

        let hcsparams1 = platform.read32(HCSPARAMS1);
        let hccparams1 = platform.read32(HCCPARAMS1);
        let max_slots = (hcsparams1 & 0xff) as u8;
        let max_ports = (hcsparams1 >> 24) as u8;
        let max_interrupters = ((hcsparams1 >> 8) & 0x7ff) as u16;
        let context_stride = if hccparams1 & HCCPARAMS1_CSZ != 0 {
            64
        } else {
            32
        };
        if max_slots == 0 || max_interrupters == 0 {
            return Err("xHCI reports no device slots or interrupters");
        }
        if hccparams1 & HCCPARAMS1_AC64 == 0 {
            return Err("xHCI without 64-bit DMA is not supported");
        }

        // RTSOFF is 32-byte aligned; it is not required to be page aligned.
        let runtime_offset = (platform.read32(RTSOFF) & 0xffff_ffe0) as usize;
        let doorbell_offset = (platform.read32(DBOFF) & 0xffff_fffc) as usize;
        debug!(
            "xhci: caplength={:#x} runtime={:#x} doorbell={:#x} slots={} interrupters={}",
            caplength, runtime_offset, doorbell_offset, max_slots, max_interrupters
        );

        let mut host = Self {
            platform: Box::new(platform),
            operational_offset: caplength,
            runtime_offset,
            doorbell_offset,
            max_slots,
            max_ports,
            max_interrupters,
            controller_context_stride: context_stride,
            command_enqueue: 0,
            command_cycle: true,
            event_dequeue: 0,
            event_cycle: true,
            registered_waiters: NoIrqLock::new(Vec::new()),
            devices: NoIrqLock::new(Vec::new()),
            dcbaa: DmaMemory::new(DMA_PAGE_SIZE).ok_or("cannot allocate xHCI DCBAA")?,
            command_ring: DmaMemory::new(DMA_PAGE_SIZE)
                .ok_or("cannot allocate xHCI command ring")?,
            event_ring: DmaMemory::new(DMA_PAGE_SIZE).ok_or("cannot allocate xHCI event ring")?,
            erst: DmaMemory::new(DMA_PAGE_SIZE).ok_or("cannot allocate xHCI ERST")?,
        };

        host.initialize_rings()?;
        host.initialize_controller()?;
        info!(
            "xhci: host initialized, command={:#x}, event={:#x}",
            host.command_ring.phys_addr(),
            host.event_ring.phys_addr()
        );
        Ok(host)
    }

    fn initialize_rings(&mut self) -> Result<(), &'static str> {
        if self.command_ring.phys_addr() & 0x3f != 0
            || self.event_ring.phys_addr() & 0x3f != 0
            || self.dcbaa.phys_addr() & 0x3f != 0
            || self.erst.phys_addr() & 0x3f != 0
        {
            return Err("xHCI DMA allocation is not 64-byte aligned");
        }

        // A one-page segment is 4 KiB aligned and cannot cross a 64 KiB
        // boundary. The final command TRB is the Link TRB.
        unsafe {
            let link = self
                .command_ring
                .as_ptr::<Trb>((TRBS_PER_RING - 1) * TRB_SIZE);
            write_volatile(
                link,
                Trb {
                    parameter: self.command_ring.phys_addr() as u64,
                    status: 0,
                    control: TRB_TYPE_LINK | TRB_LINK_TOGGLE_CYCLE | TRB_CYCLE,
                },
            );

            let erst = self.erst.as_ptr::<ErstEntry>(0);
            write_volatile(
                erst,
                ErstEntry {
                    segment_base: self.event_ring.phys_addr() as u64,
                    segment_size: TRBS_PER_RING as u32,
                    reserved: 0,
                },
            );
        }

        // Newly allocated pages are zeroed. Clean all structures after the
        // software writes so the controller sees the initial ring state.
        self.dcbaa.clean_for_device();
        self.command_ring.clean_for_device();
        self.event_ring.clean_for_device();
        self.erst.clean_for_device();
        Ok(())
    }

    fn initialize_controller(&mut self) -> Result<(), &'static str> {
        // USBSTS is the only operational register allowed while CNR=1.
        self.wait_until(|host| host.op_read32(USBSTS) & USBSTS_CNR == 0, "CNR clear")?;

        // Stop first. R/S is asynchronous; HCHalted is the completion
        // indication before changing the ring configuration.
        let command = self.op_read32(USBCMD);
        let safe_command = command & USBCMD_SAFE_WRITE_MASK;
        if command & USBCMD_RUN_STOP != 0 {
            self.op_write32(USBCMD, safe_command & !USBCMD_RUN_STOP);
        }
        self.wait_until(
            |host| host.op_read32(USBSTS) & USBSTS_HCHALTED != 0,
            "xHCI halt",
        )?;
        self.clear_usb_status_w1c();

        let page_size = self.op_read32(PAGESIZE);
        if page_size & 1 == 0 {
            return Err("xHCI does not support 4 KiB pages");
        }

        // CONFIG.MaxSlotsEn is intentionally one for this first stage.
        let config = self.op_read32(CONFIG);
        self.op_write32(CONFIG, (config & !0xff) | u32::from(self.max_slots.min(1)));
        self.op_write64(DCBAAP, self.dcbaa.phys_addr() as u64);
        self.op_write64(
            CRCR,
            (self.command_ring.phys_addr() as u64) | TRB_CYCLE as u64,
        );

        // Enable interrupter 0. wait_event_irq still performs an initial poll;
        // enabling the bits here also makes software-submitted IOC TRBs valid
        // interrupt sources when an interrupt handler is added.
        // IP is write-one-to-clear. Clear a stale request while enabling IE.
        self.enable_interrupter();
        // ERSTSZ[31:16] is RsvdP; preserve it instead of writing a bare word.
        let erst_size = self.rt_read32(ERSTSZ);
        self.rt_write32(ERSTSZ, (erst_size & !0xffff) | 1);
        self.rt_write64(ERSTBA, self.erst.phys_addr() as u64);
        self.rt_write64(ERDP, self.event_ring.phys_addr() as u64);

        // The ring pointers and all TRBs are now visible to the controller.
        self.dcbaa.clean_for_device();
        self.command_ring.clean_for_device();
        self.event_ring.clean_for_device();
        self.erst.clean_for_device();

        self.op_write32(
            USBCMD,
            (safe_command & !USBCMD_RUN_STOP) | USBCMD_RUN_STOP | USBCMD_INTERRUPT_ENABLE,
        );
        self.wait_until(
            |host| host.op_read32(USBSTS) & USBSTS_HCHALTED == 0,
            "xHCI run",
        )?;
        info!(
            "xhci: ready: PAGESIZE={:#x}, slots={}, ports={}, interrupters={}",
            page_size, self.max_slots, self.max_ports, self.max_interrupters
        );
        Ok(())
    }

    fn wait_until(
        &self,
        mut ready: impl FnMut(&Self) -> bool,
        what: &str,
    ) -> Result<(), &'static str> {
        for _ in 0..WAIT_LOOPS {
            if ready(self) {
                return Ok(());
            }
            core::hint::spin_loop();
        }
        error!("xhci: timeout while waiting for {}", what);
        Err("xHCI controller state transition timed out")
    }

    fn op_read32(&self, offset: usize) -> u32 {
        dma_read_barrier();
        self.platform.read32(self.operational_offset + offset)
    }

    fn op_write32(&self, offset: usize, value: u32) {
        dma_write_barrier();
        self.platform
            .write32(self.operational_offset + offset, value)
    }

    fn op_write64(&self, offset: usize, value: u64) {
        dma_write_barrier();
        self.platform
            .write64(self.operational_offset + offset, value)
    }

    fn rt_read32(&self, offset: usize) -> u32 {
        dma_read_barrier();
        self.platform.read32(self.runtime_offset + offset)
    }

    fn rt_write32(&self, offset: usize, value: u32) {
        dma_write_barrier();
        self.platform.write32(self.runtime_offset + offset, value)
    }

    fn rt_write64(&self, offset: usize, value: u64) {
        dma_write_barrier();
        self.platform.write64(self.runtime_offset + offset, value)
    }

    /// Clear only the USBSTS fields whose specification says RW1C.
    ///
    /// The read-back is also the posted-MMIO write flush used by legacy PCI
    /// INTx systems. HCE is read-only and is deliberately not included here:
    /// it requires a controller reset and reinitialization.
    pub(crate) fn clear_usb_status_w1c(&self) -> u32 {
        let status = self.op_read32(USBSTS);
        if status & USBSTS_HOST_CONTROLLER_ERROR != 0 {
            error!("xhci: USBSTS reports Host Controller Error");
        }
        if status & USBSTS_HOST_SYSTEM_ERROR != 0 {
            error!("xhci: USBSTS reports Host System Error");
        }

        let clear = status & USBSTS_W1C_MASK;
        if clear == 0 {
            return status;
        }
        self.op_write32(USBSTS, clear);
        let _ = self.op_read32(USBSTS);
        status
    }

    /// Enable IMAN.IE and acknowledge a stale IMAN.IP without touching any
    /// other field. IP is RW1C, while IE is ordinary RW.
    pub(crate) fn enable_interrupter(&self) {
        let iman = self.rt_read32(IMAN);
        self.rt_write32(IMAN, iman | IMAN_INTERRUPT_ENABLE);
        let _ = self.rt_read32(IMAN);
    }
    /// Consume and dispatch all event TRBs currently published by the
    /// controller.
    ///
    /// USBSTS.PCD is handled as a PORTSC notification; EINT/IMAN.IP is
    /// handled as an event-ring interrupt. A port event is dispatched before
    /// the next TRB is consumed, so a reset-generated PRC event cannot be
    /// delayed into a separate IRQ round.
    pub(crate) fn process_event_irq(&mut self) {
        let usb_status = self.clear_usb_status_w1c();
        let iman = self.rt_read32(IMAN);
        let event_ring_pending =
            usb_status & USBSTS_EVENT_INTERRUPT != 0 || iman & IMAN_INTERRUPT_PENDING != 0;
        let port_status_pending = usb_status & USBSTS_PORT_CHANGE_DETECT != 0;

        debug!(
            "xhci: irq source: USBSTS={:#010x}, IMAN={:#010x}, event-ring={}, port-status={}",
            usb_status, iman, event_ring_pending, port_status_pending
        );

        // USBSTS.PCD is only a root-hub notification. It has no TRB payload.
        // Consume the event ring only when EINT/IMAN.IP says that an event TRB
        // is pending; otherwise scan PORTSC directly for the notification.
        if !event_ring_pending {
            if port_status_pending {
                self.process_port_status_notification();
            }
            return;
        }

        while let Some(trb) = self.take_next_event() {
            debug!(
                "xhci: event-ring TRB type={}, cycle={}, parameter={:#018x}",
                trb.trb_type(),
                trb.cycle(),
                trb.parameter
            );
            let event = match event::XhciEvent::from_trb(trb) {
                Ok(event) => event,
                Err(reason) => {
                    error!("xhci: {} (TRB type {})", reason, trb.trb_type());
                    continue;
                }
            };

            match event {
                event::XhciEvent::PortStatusChange(event) => {
                    let Some(event) = self.read_port_status(event.port_id) else {
                        debug!(
                            "xhci: ignore stale port-status event for port {}",
                            event.port_id
                        );
                        continue;
                    };
                    self.handle_port_status_change_event(event);
                }
                event::XhciEvent::CommandCompletion(command) => {
                    let waiter = self.registered_waiters.lock(|waiters| {
                        waiters
                            .iter()
                            .cloned()
                            .find(|waiter| waiter.matches(&event))
                    });
                    let Some(waiter) = waiter else {
                        warn!("xhci: discard event without registered waiter: {:?}", event);
                        continue;
                    };
                    let signal = waiter.clone();
                    signal.complete();
                    // Internal enumeration commands do not call
                    // wait_event_loop, so remove every completed waiter
                    // here. The public wait path may unregister again;
                    // that second removal is harmless.
                    self.unregister_waiter(&signal);

                    // Enable Slot allocates the slot; it cannot be matched
                    // by slot_id until this completion has been processed.
                    // The enumerating device therefore owns the whole
                    // Enable Slot -> Address Device sequence.
                    let device_index = self
                        .devices
                        .lock(|devices| devices.iter().position(|dev| dev.is_enumerating()));
                    let Some(index) = device_index else {
                        warn!(
                            "xhci: command completion has no matching device state: slot={}",
                            command.slot_id
                        );
                        continue;
                    };

                    // Let the device own its state transition and the next
                    // command. Remove it only while it has mutable access to
                    // the host; reinsert it before ringing the doorbell.
                    let mut device = self.devices.lock(|devices| devices.remove(index));
                    let next_doorbell =
                        device.translate_state(self, device::XhciDeviceEvent::Command(command));
                    let released = device.is_disabled();
                    if !released {
                        self.devices.lock(|devices| devices.insert(index, device));
                    } else {
                        // Disable Slot completed, so dropping the device now
                        // releases its contexts and EP0 transfer ring.
                        drop(device);
                    }
                    match next_doorbell {
                        Ok(true) => self.bite_dorbell(),
                        Ok(false) => {}
                        Err(reason) => error!("xhci: device state transition failed: {}", reason),
                    }
                }
                event::XhciEvent::Transfer(transfer) => {
                    let transfer_event = event::XhciEvent::Transfer(transfer);
                    let waiter = self.registered_waiters.lock(|waiters| {
                        waiters
                            .iter()
                            .cloned()
                            .find(|waiter| waiter.matches(&transfer_event))
                    });
                    let Some(waiter) = waiter else {
                        warn!(
                            "xhci: discard transfer event without registered waiter: slot={}, endpoint={}",
                            transfer.slot_id, transfer.endpoint_id
                        );
                        continue;
                    };
                    let signal = waiter.clone();
                    signal.complete();
                    self.unregister_waiter(&signal);

                    let device_index = self.devices.lock(|devices| {
                        devices
                            .iter()
                            .position(|device| device.slot_id == Some(transfer.slot_id))
                    });
                    let Some(index) = device_index else {
                        warn!(
                            "xhci: transfer event has no matching device: slot={}, endpoint={}",
                            transfer.slot_id, transfer.endpoint_id
                        );
                        continue;
                    };

                    // The slot ID is the event's device identity. The device
                    // then validates the endpoint and advances its own state.
                    let mut device = self.devices.lock(|devices| devices.remove(index));
                    let result =
                        device.translate_state(self, device::XhciDeviceEvent::Transfer(transfer));
                    self.devices.lock(|devices| devices.insert(index, device));
                    if let Err(reason) = result {
                        error!("xhci: device transfer failed: {}", reason);
                    }
                }
                event::XhciEvent::HostController(event) => self.handle_host_controller_event(event),
            }
        }
    }

    fn process_port_status_notification(&mut self) {
        for port_id in 1..=self.max_ports {
            let Some(event) = self.read_port_status(port_id) else {
                continue;
            };
            self.handle_port_status_change_event(event);
        }
    }
    pub(crate) fn name(&self) -> &'static str {
        self.platform.name()
    }

    pub(crate) fn event_irq_process(&mut self) {
        self.process_event_irq();
    }

    fn handle_host_controller_event(&mut self, event: event::HostControllerEvent) {
        error!("xhci: host-controller event: {:?}", event.completion_code);
    }

    fn handle_port_status_change_event(&mut self, event: event::PortStatusChangeEvent) {
        let connection = if event.ccs {
            "connected"
        } else {
            "disconnected"
        };
        info!(
            "xhci: port {} {}: speed={:?}, ccs={}, csc={}, ped={}, pls={:?}, pp={}, portsc={:#010x}",
            event.port_id,
            connection,
            event.speed,
            event.ccs,
            event.csc,
            event.ped,
            event.link_state,
            event.powered,
            event.portsc,
        );

        if event.ccs && event.csc {
            self.handle_connect_event(event);
        } else {
            self.handle_disconnect_event(event);
        }
    }

    /// Start enumeration for a newly connected device.
    fn handle_connect_event(&mut self, event: event::PortStatusChangeEvent) {
        if self
            .devices
            .lock(|devices| devices.iter().any(|device| device.is_enumerating()))
        {
            warn!(
                "xhci: port {} change ignored while another device is enumerating",
                event.port_id
            );
            return;
        }

        let reset = match self.reset_port(event.port_id) {
            Ok(reset) => reset,
            Err(reason) => {
                error!("xhci: port {} reset failed: {}", event.port_id, reason);
                return;
            }
        };

        // The device owns the rest of enumeration. It is inserted before
        // ringing the command doorbell, so its state is visible when the
        // Enable Slot completion IRQ arrives.
        let device =
            device::XhciDevice::new(reset.port_id, reset.speed, self.controller_context_stride);
        self.devices.lock(|devices| {
            devices.retain(|old| old.port_id != device.port_id);
            devices.push(device);
        });
        let _waiter = self.send_command(event::XhciCommand::EnableSlot);
        self.bite_dorbell();
    }

    /// Stop the slot and release all resources after a disconnect.
    fn handle_disconnect_event(&mut self, event: event::PortStatusChangeEvent) {
        if !event.ccs {
            let device_index = self.devices.lock(|devices| {
                devices
                    .iter()
                    .position(|device| device.port_id == event.port_id)
            });

            let Some(index) = device_index else {
                return;
            };

            let mut device = self.devices.lock(|devices| devices.remove(index));
            let disable_command = match device.disable_slot() {
                Ok(command) => command,
                Err(reason) => {
                    // Enable Slot may not have assigned a slot yet. There is
                    // no controller-owned context to disable in that case.
                    if device.slot_id.is_none() {
                        drop(device);
                    } else {
                        error!(
                            "xhci: port {} cannot disable slot: {}",
                            event.port_id, reason
                        );
                        self.devices.lock(|devices| devices.insert(index, device));
                    }
                    return;
                }
            };
            self.devices.lock(|devices| devices.insert(index, device));
            let _waiter = self.send_command(disable_command);
            self.bite_dorbell();
            return;
        }

        panic!(
            "xhci: unsupported port status event: port={}, portsc={:#010x}",
            event.port_id, event.portsc
        );
    }
}
