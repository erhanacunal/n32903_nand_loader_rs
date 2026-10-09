ENTRY(Reset_Go)

MEMORY
{
    RAM (rwx) : ORIGIN = 0x00700000, LENGTH = 0x00800000
}

SECTIONS
{
    . = ORIGIN(RAM);

    .text :
    {
        PROVIDE(__image_start = .);
        PROVIDE(__text_start = .);

        PROVIDE(__vectors_start = .);
        KEEP(*(.vectors));
        . = ALIGN(4);
        PROVIDE(__vectors_end = .);

        KEEP(*(.init));
        . = ALIGN(4);
        *(.text .text.*);
        . = ALIGN(4);
        *(.rodata .rodata.*);
        . = ALIGN(4);
        *(.ARM.exidx .ARM.exidx.*);
        . = ALIGN(4);

        etext = .;
        PROVIDE(__text_end = .);
    } > RAM

    . = ALIGN(4);
    _etext = .;
    PROVIDE(etext = .);

    .data : AT (_etext)
    {
        PROVIDE(__data_start__ = .);
        _data = .;
        *(.data .data.*)
        . = ALIGN(4);
        PROVIDE(__data_end__ = .);
    } > RAM

    . = ALIGN(4);
    _edata = .;
    PROVIDE(edata = .);

    sbss = .;
    .bss :
    {
        PROVIDE(__bss_start__ = .);
        *(.bss .bss.*)
        *(COMMON)
        . = ALIGN(4);
        PROVIDE(__bss_end__ = .);
    } > RAM
    bss_size = __bss_end__ - __bss_start__;

    . = ALIGN(4);
    ebss = .;

    . = ALIGN(8);
    .heap :
    {
        PROVIDE(__heap_start__ = .);
        /* No heap allocation needed */
        end = .;
        PROVIDE(__heap_end__ = .);
    } > RAM

    __end__ = .;
    _stack_start = ORIGIN(RAM) + LENGTH(RAM);
}
