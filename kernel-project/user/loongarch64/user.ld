/* PLV3 静态 ET_EXEC：虚拟地址从 0x10000 起，和内核用户映射约定一致。 */
OUTPUT_ARCH(loongarch)
ENTRY(_start)
BASE = 0x10000;
SECTIONS
{
  . = BASE;
  .text : { *(.text.entry) *(.text*) }
  .rodata : { *(.rodata*) }
  .data : { *(.data*) }
  .bss : { *(.bss*) *(COMMON) }
  /* 裸机用户程序不做异常展开。 */
  /DISCARD/ : { *(.eh_frame*) *(.comment*) }
}
