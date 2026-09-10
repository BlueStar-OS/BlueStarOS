//! Common xHCI host-controller protocol support.

use crate::arch::riscv64::driver::dma::{dma_read_barrier, dma_write_barrier, DmaMemory};
use crate::sync::NoIrqLock;
use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ptr::write_volatile;
use lazy_static::lazy_static;
use log::{debug, error, info, warn};

pub mod descriptor;
pub mod device;
pub mod driver;
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
    let mut host = Box::new(host);
    info!("xhci: register host {}", host.platform.name());
    XHCI_HOSTS.lock(|hosts| {
        // This minimal sysfs view uses one bus number per controller. It is
        // deliberately not a full USB topology implementation yet.
        host.bus_number = (hosts.len() + 1) as u8;
        hosts.push(host);
    });
}

/// Dispatch the xHCI PLIC source to every registered host.
fn event_irq_process() {
    XHCI_HOSTS.lock(|hosts| {
        for host in hosts.iter_mut() {
            host.process_event_irq();
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
    /// One-based controller index exported as the minimal USB bus number.
    bus_number: u8,
    controller_context_stride: usize,
    command_enqueue: usize,
    command_cycle: bool,
    event_dequeue: usize,
    event_cycle: bool,
    command_routes: Vec<Option<device::DeviceId>>,
    pub(crate) devices: NoIrqLock<Vec<device::XhciDeviceRef>>,
    next_device_id: u32,
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
            bus_number: 0,
            controller_context_stride: context_stride,
            command_enqueue: 0,
            command_cycle: true,
            event_dequeue: 0,
            event_cycle: true,
            command_routes: (0..TRBS_PER_RING - 1).map(|_| None).collect(),
            devices: NoIrqLock::new(Vec::new()),
            next_device_id: 1,
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

        // Enable interrupter 0 before the controller can publish an event.
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
                    match self.take_command_owner(command.command_trb_pointer) {
                        Some(id) => self.dispatch_device_command_completion(id, command),
                        None => warn!(
                            "xhci: discard command completion without owner: pointer={:#x}",
                            command.command_trb_pointer
                        ),
                    }
                }
                event::XhciEvent::Transfer(transfer) => {
                    self.dispatch_device_transfer_completion(transfer);
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

    fn allocate_device_id(&mut self) -> device::DeviceId {
        let id = device::DeviceId(self.next_device_id);
        self.next_device_id = self.next_device_id.wrapping_add(1).max(1);
        id
    }

    fn take_command_owner(&mut self, pointer: u64) -> Option<device::DeviceId> {
        let base = self.command_ring.phys_addr() as u64;
        let bytes = u64::try_from(TRB_SIZE).ok()?;
        let offset = pointer.checked_sub(base)?;
        if offset % bytes != 0 {
            return None;
        }
        let index = usize::try_from(offset / bytes).ok()?;
        let index = (index < TRBS_PER_RING - 1).then_some(index)?;
        self.command_routes[index].take()
    }

    pub(crate) fn set_command_owner(&mut self, index: usize, id: device::DeviceId) {
        assert!(index < self.command_routes.len());
        assert!(
            self.command_routes[index].is_none(),
            "xHCI command ring route overwrite"
        );
        self.command_routes[index] = Some(id);
    }

    fn submit_device_command(&mut self, id: device::DeviceId, command: event::XhciCommand) {
        self.enqueue_command(command, id);
        self.bite_dorbell();
    }

    fn find_device_by_id(&self, id: device::DeviceId) -> Option<device::XhciDeviceRef> {
        self.devices.lock(|devices| {
            devices
                .iter()
                .find(|device| device.lock(|device| device.id == id))
                .cloned()
        })
    }

    fn find_device_by_slot(&self, slot_id: u8) -> Option<device::XhciDeviceRef> {
        self.devices.lock(|devices| {
            devices
                .iter()
                .find(|device| device.lock(|device| device.slot_id == Some(slot_id)))
                .cloned()
        })
    }

    fn find_device_by_port(&self, port_id: u8) -> Option<device::XhciDeviceRef> {
        self.devices.lock(|devices| {
            devices
                .iter()
                .find(|device| device.lock(|device| device.port_id == port_id))
                .cloned()
        })
    }

    fn dispatch_device_command_completion(
        &mut self,
        id: device::DeviceId,
        command: event::CommandCompletionEvent,
    ) {
        let Some(device) = self.find_device_by_id(id) else {
            error!("xhci: command completion references a removed device");
            return;
        };
        let action = device
            .lock(|device| device.on_command_completion(command, self.max_slots, &self.dcbaa));
        match action {
            Ok(action) => self.execute_device_action(id, action),
            Err(reason) => error!("xhci: device command completion failed: {}", reason),
        }
    }

    fn dispatch_device_transfer_completion(&mut self, transfer: event::TransferEvent) {
        let Some(device) = self.find_device_by_slot(transfer.slot_id) else {
            error!(
                "xhci: xHCI transfer event has no matching device: slot={}",
                transfer.slot_id
            );
            return;
        };
        let result = device.lock(|device| {
            let id = device.id;
            device
                .on_transfer_completion(transfer)
                .map(|action| (id, action))
        });
        match result {
            Ok((id, action)) => self.execute_device_action(id, action),
            Err(reason) => error!(
                "xhci: device transfer completion failed: slot={}, endpoint={}, {}",
                transfer.slot_id, transfer.endpoint_id, reason
            ),
        }
    }

    fn execute_device_action(&mut self, id: device::DeviceId, action: device::DeviceAction) {
        match action {
            device::DeviceAction::SubmitCommand(command) => self.submit_device_command(id, command),
            device::DeviceAction::RingEndpoint { dci } => {
                let slot_id = self
                    .find_device_by_id(id)
                    .and_then(|device| device.lock(|device| device.slot_id));
                let Some(slot_id) = slot_id else {
                    error!("xhci: device action has no xHCI slot");
                    return;
                };
                self.ring_device_doorbell(slot_id, dci);
            }
            device::DeviceAction::RecordInSys => {
                let Some(device) = self.find_device_by_id(id) else {
                    error!("xhci: configured device was removed before driver probe");
                    return;
                };
                let record = device.lock(|device| device.sys_record(self.bus_number));
                if let Some(record) = record {
                    crate::fs::sys::dev::record_usb_device(record);
                } else {
                    error!("xhci: configured device cannot produce a sysfs record");
                }
                device::XhciDevice::try_find_and_register_driver(device);
            }
            device::DeviceAction::Release => {
                let Some(device) = self.find_device_by_id(id) else {
                    return;
                };
                let port_id = device.lock(|device| device.port_id);
                crate::fs::sys::dev::remove_usb_device(self.bus_number, port_id);
                self.devices
                    .lock(|devices| devices.retain(|candidate| !Arc::ptr_eq(candidate, &device)));
            }
        }
    }

    fn ring_device_doorbell(&self, slot_id: u8, dci: u8) {
        if slot_id == 0 || slot_id > self.max_slots || dci == 0 || dci > 31 {
            error!(
                "xhci: invalid device doorbell slot={}, dci={}",
                slot_id, dci
            );
            return;
        }
        dma_write_barrier();
        self.platform.write32(
            self.doorbell_offset + usize::from(slot_id) * 4,
            u32::from(dci),
        );
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
        let id = self.allocate_device_id();
        let device = device::XhciDevice::new(
            id,
            reset.port_id,
            reset.speed,
            self.controller_context_stride,
        );
        let device = Arc::new(NoIrqLock::new(device));
        self.devices.lock(|devices| {
            devices.retain(|old| old.lock(|old| old.port_id != reset.port_id));
            devices.push(device);
        });
        crate::fs::sys::dev::remove_usb_device(self.bus_number, reset.port_id);
        self.submit_device_command(id, event::XhciCommand::EnableSlot);
    }

    /// Stop the slot and release all resources after a disconnect.
    fn handle_disconnect_event(&mut self, event: event::PortStatusChangeEvent) {
        if !event.ccs {
            // Clear the latched disconnect/change bits without writing zero
            // to PORTSC's power and link-control fields.
            let acknowledge = event::portsc_event_ack_value(event.portsc);
            if acknowledge != 0 {
                let portsc_offset =
                    PORT_REGISTER_BASE + (usize::from(event.port_id) - 1) * PORT_REGISTER_STRIDE;
                self.op_write32(portsc_offset, acknowledge);
            }
            crate::fs::sys::dev::remove_usb_device(self.bus_number, event.port_id);
            let Some(device) = self.find_device_by_port(event.port_id) else {
                return;
            };

            let disable_command = device.lock(|device| device.disable_slot());
            let disable_command = match disable_command {
                Ok(command) => command,
                Err(reason) => {
                    // Enable Slot may not have assigned a slot yet. There is
                    // no controller-owned context to disable in that case.
                    let has_slot = device.lock(|device| device.slot_id.is_some());
                    if !has_slot {
                        self.devices.lock(|devices| {
                            devices.retain(|candidate| !Arc::ptr_eq(candidate, &device));
                        });
                    } else {
                        error!(
                            "xhci: port {} cannot disable slot: {}",
                            event.port_id, reason
                        );
                    }
                    return;
                }
            };
            let id = device.lock(|device| device.id);
            self.submit_device_command(id, disable_command);
            return;
        }

        panic!(
            "xhci: unsupported port status event: port={}, portsc={:#010x}",
            event.port_id, event.portsc
        );
    }
}
