OUTPUT_ARCH(riscv)
ENTRY(_start)
BASE = 0x10000;
SECTIONS
{
  . = BASE;
  .text : { *(.text.entry) *(.text*) }
  .rodata : { *(.rodata*) }
  .data : { *(.data*) }
  .bss : { *(.bss*) *(COMMON) }
  /DISCARD/ : { *(.eh_frame*) *(.comment) *(.riscv.attributes) }
}