OUTPUT_ARCH(aarch64)
ENTRY(_blue_start)

/* U-Boot loads kernel to 0x02000000 */
/* BASE_ADDRESS = 0x40080000; */
BASE_ADDRESS = 0x02000000;
SECTIONS
{
    . = BASE_ADDRESS;
    skernel = .;

    stext = .;
    .text : {
        *(.text.entry)
        . = ALIGN(4K);
        straper = .;
        *(.text.traper)
        . = ALIGN(4K);
        *(.text .text.*)
    }

    . = ALIGN(4K);
    etext = .;
    srodata = .;
    .rodata : {
        *(.rodata .rodata.*)
        *(.srodata .srodata.*)
    }

    . = ALIGN(4K);
    erodata = .;
    sdata = .;
    .data : {
        *(.data.app)
        *(.data .data.*)
        *(.sdata .sdata.*)
    }

    . = ALIGN(4K);
    edata = .;
    .bss : {
        sbss = .;
        *(.bss.stack)
        estack = .;
        *(.bss .bss.*)
        *(.sbss .sbss.*)
    }

    . = ALIGN(4K);
    ebss = .;

    /* DTB probe entries (按优先级排序) */
    . = ALIGN(8);
    __dtb_probe_start = .;
    .dtb_probe : {
        KEEP(*(.dtb_probe.high))
        KEEP(*(.dtb_probe.mid))
        KEEP(*(.dtb_probe.low))
    }
    __dtb_probe_end = .;

    . = ALIGN(4K);
    ekernel = .;

    _kernel_size = ekernel - skernel;

    /DISCARD/ : {
        *(.eh_frame)
    }
}
