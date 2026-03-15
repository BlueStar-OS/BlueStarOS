OUTPUT_ARCH(aarch64)
ENTRY(_start)
BASE_ADDRESS = 0x2000;

SECTIONS
{
    . = BASE_ADDRESS;
    stext = .;

    .text : {
        *(.text.entry)
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
        *(.data .data.*)
        *(.sdata .sdata.*)
    }

    . = ALIGN(4K);
    edata = .;
    .bss : {
        sbss = .;
        *(.bss.stack)
        *(.bss .bss.*)
        *(.sbss .sbss.*)
        ebss = .;
    }

    /DISCARD/ : {
        *(.eh_frame)
    }
}
