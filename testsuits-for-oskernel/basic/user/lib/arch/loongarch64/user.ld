OUTPUT_ARCH(loongarch64)
ENTRY(_start)

SECTIONS {

    .text : { 
        *(.text.entry)
        *(.text .text.*)
    }

    .bss : { 
        *(.bss .bss.*)
        *(.sbss .sbss.*)
    }

    .data : { 
        *(.data .rodata) 
        *(.sdata .sdata.*)
    }
    
    /DISCARD/ : { *(.eh_*) }
}