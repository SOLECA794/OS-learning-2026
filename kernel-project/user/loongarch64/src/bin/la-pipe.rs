#![no_std]
#![no_main]

//! LoongArch pipe 验收：失败不泄漏 FD，读写与 stdout 重定向经过 FD 表。

use core::arch::global_asm;

global_asm!(
    r#"
    .section .text.entry, "ax"
    .globl _start
_start:
    # 本地 8 字节 FD 对和 16 字节读缓冲。
    addi.d $r3, $r3, -32

    # 无效用户指针须返回 EFAULT，不能消耗 FD 3/4。
    li.w $r11, 59
    move $r4, $r0
    syscall 0
    li.w $r12, -14
    bne $r4, $r12, .Lfail
    li.w $r11, 59
    move $r4, $r3
    syscall 0
    bnez $r4, .Lfail
    ld.w $r23, $r3, 0
    ld.w $r24, $r3, 4
    li.w $r12, 3
    bne $r23, $r12, .Lfail
    li.w $r12, 4
    bne $r24, $r12, .Lfail

    # 普通管道写端写入 5 字节，读端按原顺序读回。
    li.w $r11, 64
    move $r4, $r24
    la.local $r5, .Lhello
    li.w $r6, 5
    syscall 0
    li.w $r12, 5
    bne $r4, $r12, .Lfail
    li.w $r11, 63
    move $r4, $r23
    addi.d $r5, $r3, 16
    li.w $r6, 8
    syscall 0
    li.w $r12, 5
    bne $r4, $r12, .Lfail
    ld.b $r12, $r3, 16
    li.w $r13, 104
    bne $r12, $r13, .Lfail
    ld.b $r12, $r3, 20
    li.w $r13, 111
    bne $r12, $r13, .Lfail

    # 保存原 stdout，重定向 fd 1 到管道，再恢复 stdout。
    li.w $r11, 23
    li.w $r4, 1
    syscall 0
    blt $r4, $r0, .Lfail
    move $r25, $r4
    li.w $r11, 24
    move $r4, $r24
    li.w $r5, 1
    syscall 0
    li.w $r12, 1
    bne $r4, $r12, .Lfail
    li.w $r11, 64
    li.w $r4, 1
    la.local $r5, .Lworld
    li.w $r6, 5
    syscall 0
    li.w $r12, 5
    bne $r4, $r12, .Lfail
    li.w $r11, 24
    move $r4, $r25
    li.w $r5, 1
    syscall 0
    li.w $r12, 1
    bne $r4, $r12, .Lfail
    li.w $r11, 57
    move $r4, $r25
    syscall 0
    bnez $r4, .Lfail
    li.w $r11, 63
    move $r4, $r23
    addi.d $r5, $r3, 16
    li.w $r6, 8
    syscall 0
    li.w $r12, 5
    bne $r4, $r12, .Lfail
    ld.b $r12, $r3, 16
    li.w $r13, 119
    bne $r12, $r13, .Lfail
    ld.b $r12, $r3, 20
    li.w $r13, 100
    bne $r12, $r13, .Lfail

    # 关闭写端后空管道读取返回 0；读端随后也关闭。
    li.w $r11, 57
    move $r4, $r24
    syscall 0
    bnez $r4, .Lfail
    li.w $r11, 63
    move $r4, $r23
    addi.d $r5, $r3, 16
    li.w $r6, 8
    syscall 0
    bnez $r4, .Lfail
    li.w $r11, 57
    move $r4, $r23
    syscall 0
    bnez $r4, .Lfail

    # 所有断言成功后，已恢复的 stdout 输出唯一通过标记。
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
.Lhello:
    .ascii "hello"
.Lworld:
    .ascii "world"
.Lmessage:
    .ascii "LA_PIPE_PASS\n"
"#
);

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
