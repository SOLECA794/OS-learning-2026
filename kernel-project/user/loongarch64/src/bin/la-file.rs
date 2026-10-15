#![no_std]
#![no_main]

//! LoongArch 文件描述符验收：ramfs/staticfs 的 openat/read/close 和失败边界。

use core::arch::global_asm;

global_asm!(
    r#"
    .section .text.entry, "ax"
    .globl _start
_start:
    # 留一块用户栈缓冲，故障读后确认文件偏移仍为零。
    addi.d $r3, $r3, -32
    li.w $r11, 56
    li.w $r4, -100
    la.local $r5, .Lramfs_path
    move $r6, $r0
    move $r7, $r0
    syscall 0
    blt $r4, $r0, .Lfail
    move $r23, $r4

    # NULL 缓冲必须返回 EFAULT，且不能消费打开描述的偏移。
    li.w $r11, 63
    move $r4, $r23
    move $r5, $r0
    li.w $r6, 1
    syscall 0
    li.w $r12, -14
    bne $r4, $r12, .Lfail

    # /hello.txt 应返回完整的 11 字节，然后 EOF。
    li.w $r11, 63
    move $r4, $r23
    move $r5, $r3
    li.w $r6, 16
    syscall 0
    li.w $r12, 11
    bne $r4, $r12, .Lfail
    ld.b $r12, $r3, 0
    li.w $r13, 104
    bne $r12, $r13, .Lfail
    ld.b $r12, $r3, 10
    li.w $r13, 115
    bne $r12, $r13, .Lfail
    li.w $r11, 63
    move $r4, $r23
    move $r5, $r3
    li.w $r6, 16
    syscall 0
    bnez $r4, .Lfail

    # close 释放 FD 槽；再次 read 应返回 EBADF。
    li.w $r11, 57
    move $r4, $r23
    syscall 0
    bnez $r4, .Lfail
    li.w $r11, 63
    move $r4, $r23
    move $r5, $r3
    li.w $r6, 1
    syscall 0
    li.w $r12, -9
    bne $r4, $r12, .Lfail

    # 第二后端通过同一 VFS 路径解析并读取 17 字节。
    li.w $r11, 56
    li.w $r4, -100
    la.local $r5, .Lstatic_path
    move $r6, $r0
    move $r7, $r0
    syscall 0
    blt $r4, $r0, .Lfail
    move $r23, $r4
    li.w $r11, 63
    move $r4, $r23
    move $r5, $r3
    li.w $r6, 32
    syscall 0
    li.w $r12, 17
    bne $r4, $r12, .Lfail
    ld.b $r12, $r3, 0
    li.w $r13, 115
    bne $r12, $r13, .Lfail
    ld.b $r12, $r3, 16
    li.w $r13, 10
    bne $r12, $r13, .Lfail
    li.w $r11, 57
    move $r4, $r23
    syscall 0
    bnez $r4, .Lfail

    # 无效路径应返回 ENOENT，且旧任务仍能继续写控制台。
    li.w $r11, 56
    li.w $r4, -100
    la.local $r5, .Lmissing_path
    move $r6, $r0
    move $r7, $r0
    syscall 0
    li.w $r12, -2
    bne $r4, $r12, .Lfail
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
.Lramfs_path:
    .asciz "/hello.txt"
.Lstatic_path:
    .asciz "/static/readme"
.Lmissing_path:
    .asciz "/la-missing"
.Lmessage:
    .ascii "LA_FILE_PASS\n"
"#
);

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
