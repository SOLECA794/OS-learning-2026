#![no_std]
#![no_main]

//! LoongArch exec 目标：检查新现场参数并发出唯一成功标记。

use core::arch::global_asm;

global_asm!(
    r#"
    .section .text.entry, "ax"
    .globl _start
_start:
    # Linux 风格入口 a0=argc、a1=argv；检查新用户栈上的指针和字符串。
    li.w $r12, 1
    bne $r4, $r12, .Lfail
    ld.d $r12, $r5, 0
    beqz $r12, .Lfail
    ld.d $r13, $r5, 8
    bnez $r13, .Lfail
    ld.b $r13, $r12, 0
    li.w $r14, 99
    bne $r13, $r14, .Lfail
    ld.b $r13, $r12, 1
    li.w $r14, 104
    bne $r13, $r14, .Lfail
    ld.b $r13, $r12, 2
    li.w $r14, 101
    bne $r13, $r14, .Lfail
    ld.b $r13, $r12, 3
    li.w $r14, 99
    bne $r13, $r14, .Lfail
    ld.b $r13, $r12, 4
    li.w $r14, 107
    bne $r13, $r14, .Lfail
    ld.b $r13, $r12, 5
    bnez $r13, .Lfail

    # 目标程序成功运行后输出标记并以 0 退出。
    li.w $r11, 64
    li.w $r4, 1
    la.local $r5, .Lmessage
    li.w $r6, 13
    syscall 0
    li.w $r12, 13
    bne $r4, $r12, .Lfail
    move $r4, $r0
    b .Lexit
.Lfail:
    li.w $r4, 1
.Lexit:
    li.w $r11, 93
    syscall 0
.Lpark:
    b .Lpark

    .section .rodata
.Lmessage:
    .ascii "LA_EXEC_PASS\n"
"#
);

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
