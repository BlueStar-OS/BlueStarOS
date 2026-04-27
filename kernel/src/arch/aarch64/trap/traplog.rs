use crate::arch::memory::VirAddr;
use crate::task::TASK_MANAER;
use alloc::string::String;
use core::arch::asm;
use log::{debug, error};
pub(crate) struct TrapDetail {
    pub esr: u64,
    pub elr: u64,
    pub far: u64,
    pub spsr: u64,
    pub sp_el0: u64,
    pub sp_el1: u64,
    pub ttbr0_el1: u64,
    pub ttbr1_el1: u64,
    pub ec: ExceptionClass,
    pub ec_code: u8,
    pub esr_detail: String,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExceptionClass {
    Unknown = 0x00,
    SVC64 = 0x15,
    InstructionAbortLower = 0x20,
    InstructionAbortSame = 0x21,
    PCAlignment = 0x22,
    DataAbortLower = 0x24,
    DataAbortSame = 0x25,
    SPAlignment = 0x26,
    BRK = 0x3C,
}

impl ExceptionClass {
    pub fn from_esr(esr: u64) -> Self {
        let ec = ((esr >> 26) & 0x3F) as u8;
        match ec {
            0x15 => Self::SVC64,
            0x20 => Self::InstructionAbortLower,
            0x21 => Self::InstructionAbortSame,
            0x22 => Self::PCAlignment,
            0x24 => Self::DataAbortLower,
            0x25 => Self::DataAbortSame,
            0x26 => Self::SPAlignment,
            0x3C => Self::BRK,
            _ => Self::Unknown,
        }
    }
}

#[inline]
fn data_fault_status_name(dfsc: u64) -> &'static str {
    match dfsc {
        0x04 => "TranslationFaultL0",
        0x05 => "TranslationFaultL1",
        0x06 => "TranslationFaultL2",
        0x07 => "TranslationFaultL3",
        0x09 => "AccessFlagFaultL1",
        0x0A => "AccessFlagFaultL2",
        0x0B => "AccessFlagFaultL3",
        0x0D => "PermissionFaultL1",
        0x0E => "PermissionFaultL2",
        0x0F => "PermissionFaultL3",
        0x10 => "SyncExternalAbort",
        0x11 => "SyncTagCheckOrExternalAbortL1",
        0x14 => "SyncExternalAbortOnWalkL0",
        0x15 => "SyncExternalAbortOnWalkL1",
        0x16 => "SyncExternalAbortOnWalkL2",
        0x17 => "SyncExternalAbortOnWalkL3",
        0x21 => "AlignmentFault",
        0x30 => "TlbConflictAbort",
        _ => "UnknownDFSC",
    }
}

#[inline]
fn instruction_fault_status_name(ifsc: u64) -> &'static str {
    match ifsc {
        0x04 => "TranslationFaultL0",
        0x05 => "TranslationFaultL1",
        0x06 => "TranslationFaultL2",
        0x07 => "TranslationFaultL3",
        0x09 => "AccessFlagFaultL1",
        0x0A => "AccessFlagFaultL2",
        0x0B => "AccessFlagFaultL3",
        0x0D => "PermissionFaultL1",
        0x0E => "PermissionFaultL2",
        0x0F => "PermissionFaultL3",
        0x10 => "SyncExternalAbort",
        0x14 => "SyncExternalAbortOnWalkL0",
        0x15 => "SyncExternalAbortOnWalkL1",
        0x16 => "SyncExternalAbortOnWalkL2",
        0x17 => "SyncExternalAbortOnWalkL3",
        0x21 => "AlignmentFault",
        0x30 => "TlbConflictAbort",
        _ => "UnknownIFSC",
    }
}

#[inline]
fn decode_esr_detail(esr: u64) -> (&'static str, u64, String) {
    let ec = ExceptionClass::from_esr(esr);
    let il = (esr >> 25) & 0x1;
    let iss = esr & 0x1FF_FFFF;
    match ec {
        ExceptionClass::DataAbortLower | ExceptionClass::DataAbortSame => {
            let dfsc = iss & 0x3F;
            let wnr = (iss >> 6) & 0x1;
            let s1ptw = (iss >> 7) & 0x1;
            let cm = (iss >> 8) & 0x1;
            (
                "DataAbort",
                dfsc,
                alloc::format!(
                    "IL={} ISS={:#x} WnR={} S1PTW={} CM={} DFSC={}({:#x})",
                    il,
                    iss,
                    wnr,
                    s1ptw,
                    cm,
                    data_fault_status_name(dfsc),
                    dfsc
                ),
            )
        }
        ExceptionClass::InstructionAbortLower | ExceptionClass::InstructionAbortSame => {
            let ifsc = iss & 0x3F;
            let s1ptw = (iss >> 7) & 0x1;
            (
                "InstructionAbort",
                ifsc,
                alloc::format!(
                    "IL={} ISS={:#x} S1PTW={} IFSC={}({:#x})",
                    il,
                    iss,
                    s1ptw,
                    instruction_fault_status_name(ifsc),
                    ifsc
                ),
            )
        }
        ExceptionClass::SVC64 => (
            "SVC",
            iss,
            alloc::format!("IL={} ISS={:#x} imm16={:#x}", il, iss, iss & 0xFFFF),
        ),
        _ => ("Other", iss, alloc::format!("IL={} ISS={:#x}", il, iss)),
    }
}

#[inline]
pub(crate) fn read_esr_el1() -> u64 {
    let esr: u64;
    unsafe {
        asm!("mrs {}, esr_el1", out(reg) esr);
    }
    esr
}

#[inline]
pub(crate) fn read_elr_el1() -> u64 {
    let elr: u64;
    unsafe {
        asm!("mrs {}, elr_el1", out(reg) elr);
    }
    elr
}

#[inline]
pub(crate) fn read_far_el1() -> u64 {
    let far: u64;
    unsafe {
        asm!("mrs {}, far_el1", out(reg) far);
    }
    far
}

#[inline]
pub(crate) fn read_spsr_el1() -> u64 {
    let spsr: u64;
    unsafe {
        asm!("mrs {}, spsr_el1", out(reg) spsr);
    }
    spsr
}

#[inline]
pub(crate) fn read_sp_el0() -> u64 {
    let value: u64;
    unsafe {
        asm!("mrs {}, sp_el0", out(reg) value);
    }
    value
}

#[inline]
pub(crate) fn read_sp_el1() -> u64 {
    let value: u64;
    unsafe {
        asm!("mov {}, sp", out(reg) value);
    }
    value
}

#[inline]
pub(crate) fn read_ttbr0_el1() -> u64 {
    let value: u64;
    unsafe {
        asm!("mrs {}, ttbr0_el1", out(reg) value);
    }
    value
}

#[inline]
pub(crate) fn read_ttbr1_el1() -> u64 {
    let value: u64;
    unsafe {
        asm!("mrs {}, ttbr1_el1", out(reg) value);
    }
    value
}

#[inline]
pub(crate) fn current_trap_detail() -> TrapDetail {
    let esr = read_esr_el1();
    let ec_code = ((esr >> 26) & 0x3F) as u8;
    let ec = ExceptionClass::from_esr(esr);
    let (_kind, _code, esr_detail) = decode_esr_detail(esr);

    TrapDetail {
        esr,
        elr: read_elr_el1(),
        far: read_far_el1(),
        spsr: read_spsr_el1(),
        sp_el0: read_sp_el0(),
        sp_el1: read_sp_el1(),
        ttbr0_el1: read_ttbr0_el1(),
        ttbr1_el1: read_ttbr1_el1(),
        ec,
        ec_code,
        esr_detail,
    }
}

impl TrapDetail {
    pub fn log_error(&self, tag: &str) {
        error!(
            "{}: EC={:?} ESR={:#x} ELR={:#x} FAR={:#x} SPSR={:#x}",
            tag, self.ec, self.esr, self.elr, self.far, self.spsr
        );
        error!(
            "{} detail: {} SP_EL0={:#x} SP_EL1={:#x} TTBR0_EL1={:#x} TTBR1_EL1={:#x}",
            tag, self.esr_detail, self.sp_el0, self.sp_el1, self.ttbr0_el1, self.ttbr1_el1
        );
    }

    pub fn log_debug(&self, tag: &str) {
        debug!(
            "{}: EC={:?}({:#x}) ESR={:#x} ELR={:#x} FAR={:#x}",
            tag, self.ec, self.ec_code, self.esr, self.elr, self.far
        );
        debug!(
            "{} detail: {} SP_EL0={:#x} SP_EL1={:#x} TTBR0_EL1={:#x} TTBR1_EL1={:#x}",
            tag, self.esr_detail, self.sp_el0, self.sp_el1, self.ttbr0_el1, self.ttbr1_el1
        );
    }
}

pub(crate) fn log_exception_detail(tag: &str) {
    current_trap_detail().log_error(tag);
}

pub(crate) fn log_unhandled_page_fault(
    fault_addr: crate::arch::memory::VirAddr,
    esr: u64,
    pte_flags: Option<crate::arch::memory::PTEFlags>,
) {
    error!("PageFault Unhandled! Killed.");
    error!("  Addr: {:#x}", fault_addr.0);
    error!("  ESR: {:#x}", esr);
    if let Some(flags) = pte_flags {
        error!("  PTE Flags: {:?}", flags);
    }
}

pub fn log_user_opcode_window(elr: u64) {
    use crate::arch::memory::PageTable;

    let satp = TASK_MANAER.get_current_stap();
    let mut pt = PageTable::crate_table_from_satp(satp);

    let cur = pt
        .read_bytes_from_userspace(VirAddr(elr as usize), 4)
        .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()));
    let prev = if elr >= 4 {
        pt.read_bytes_from_userspace(VirAddr(elr as usize - 4), 4)
            .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
    } else {
        None
    };

    error!(
        "User opcode window: ELR={:#x} prev@{:#x}={:#010x?} cur@{:#x}={:#010x?} satp={:#x}",
        elr,
        elr.saturating_sub(4),
        prev,
        elr,
        cur,
        satp
    );
}
