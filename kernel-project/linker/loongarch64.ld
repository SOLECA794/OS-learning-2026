/* LoongArch64 链接脚本（QEMU virt）
 *
 * 基址 0x00200000：QEMU loongarch virt 的第一段 RAM 是 0x0..0x10000000（低 256MB），
 * 内核必须落在这一段内。不要把内核放到 0x80000000——低 256MB 与 highmem 之间存在空洞，
 * BSS 跨越空洞会在 boot 早期产生未映射写入。
 *
 * boot 阶段 CRMD.PG=0 / DA=1（物理直址），不启用分页；DMW 配置属 M1 范畴。
 */

ENTRY(_start)

BASE_ADDRESS = 0x00200000;
BOOT_STACK_SIZE = 8K;
BOOT_HART_COUNT = 8;

SECTIONS
{
    . = BASE_ADDRESS;

    .text : {
        __text_start = .;
        *(.text.entry)
        *(.text .text.*)
        __text_end = .;
    }

    .rodata : ALIGN(4K) {
        __rodata_start = .;
        *(.rodata .rodata.*)
        *(.srodata .srodata.*)
        __rodata_end = .;
    }

    .data : ALIGN(4K) {
        __data_start = .;
        *(.data .data.*)
        *(.sdata .sdata.*)
        __data_end = .;
    }

    .bss : ALIGN(4K) {
        __bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        . = ALIGN(4K);
        __bss_end = .;
    }

    /* 同 riscv64.ld：per-hart boot 栈数组（每核 8K，共 8 核 = 64K）。 */
    .boot_stack (NOLOAD) : ALIGN(64K) {
        __boot_stack_bottom = .;
        . = . + BOOT_STACK_SIZE * BOOT_HART_COUNT;
        __boot_stack_top = .;
    }

    __kernel_end = .;

    /DISCARD/ : {
        *(.eh_frame)
        *(.note.GNU-stack)
        *(.debug_info .debug_info.*)
    }
}