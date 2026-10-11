/* RISC-V64 链接脚本
 *
 * QEMU virt + OpenSBI：固件把内核装载到 DRAM 起始偏移处（0x80000000 + 2MB），
 * 因此链接基址固定为 0x80200000。改动此值必须同步 build.mk 的 LINK_BASE。
 *
 * 入口 _start 位于 .text.entry（见 src/arch/riscv64/boot.rs 的 global_asm），
 * 必须在映像最前面，否则 OpenSBI 跳转到 ENTRY 之外的地址。
 */

OUTPUT_ARCH(riscv)
ENTRY(_start)

BASE_ADDRESS = 0x80200000;
BOOT_STACK_SIZE = 8K;
BOOT_HART_COUNT = 8;

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

    /* M0：per-hart boot 栈数组（每核 8K，共 8 核 = 64K）。
     * 每个核在汇编里按 hartid 取自己的栈，全部进入 Rust 后由
     * kernel_main 的原子锁选主（见 kernel/mod.rs）。
     * 退出条件：M1 引入 arch::setup_all_cores 后改为正式的 per-hart 栈管理，
     *           并删除本注释中的 M0 限定说明。
     */
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