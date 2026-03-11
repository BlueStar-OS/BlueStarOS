pub mod address;


pub use address::*;

use crate::memory::MapSet;



pub fn active_memset(memset:&MapSet){
        use log::debug;
        use crate::arch::riscv64::satp;
        use crate::arch::riscv64::asm;
        let satps = memset.table.satp_token();
        debug!("Active PageTable: SATP = {:#x}", satps);
        unsafe {
            // 直接用汇编写入satp，而不是用riscv crate的satp::write()
            core::arch::asm!(
                "csrw satp, {0}",
                in(reg) satps
            );
            asm!("sfence.vma");
            debug!("Page Witch Successful!!!!!");
        }
}