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
BOOT_STACK_SIZE = 64K;

SECTIONS
{
    . = BASE_ADDRESS;

    .text : {
        *(.text.entry)
        *(.text .text.*)
    }

    .rodata : ALIGN(4K) {
        *(.rodata .rodata.*)
        *(.srodata .srodata.*)
    }

    .data : ALIGN(4K) {
        *(.data .data.*)
        *(.sdata .sdata.*)
    }

    .bss : ALIGN(4K) {
        __bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        . = ALIGN(4K);
        __bss_end = .;
    }

    /* 同 riscv64.ld：M0 只预留 bootstrap hart 的 boot 栈。 */
    .boot_stack (NOLOAD) : ALIGN(64K) {
        __boot_stack_bottom = .;
        . = . + BOOT_STACK_SIZE;
        __boot_stack_top = .;
    }

    __kernel_end = .;

    /DISCARD/ : {
        *(.eh_frame)
        *(.note.GNU-stack)
        *(.debug_info .debug_info.*)
    }
}
