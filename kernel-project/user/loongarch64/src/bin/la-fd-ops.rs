#![no_std]
#![no_main]

//! LoongArch FD 第二批验收：定位 IO 不改变游标，dup 共享打开描述。

use core::arch::global_asm;

global_asm!(
    r#"
    .section .text.entry, "ax"
    .globl _start
_start:
    addi.d $r3, $r3, -32
    # 以读写模式打开 ramfs 文件；两次打开之间不依赖固定 FD 数值。
    li.w $r11, 56
    li.w $r4, -100
    la.local $r5, .Lpath
    li.w $r6, 2
    move $r7, $r0
    syscall 0
    blt $r4, $r0, .Lfail
    move $r23, $r4

    # pread 从偏移 1 读取 "el"，打开描述的当前游标仍是 0。
    li.w $r11, 67
    move $r4, $r23
    move $r5, $r3
    li.w $r6, 2
    li.w $r7, 1
    syscall 0
    li.w $r12, 2
    bne $r4, $r12, .Lfail
    ld.b $r12, $r3, 0
    li.w $r13, 101
    bne $r12, $r13, .Lfail
    ld.b $r12, $r3, 1
    li.w $r13, 108
    bne $r12, $r13, .Lfail
    li.w $r11, 62
    move $r4, $r23
    move $r5, $r0
    li.w $r6, 1
    syscall 0
    bnez $r4, .Lfail

    # dup 取得同一打开描述；通过副本读 2 字节应把原 FD 游标推进到 2。
    li.w $r11, 23
    move $r4, $r23
    syscall 0
    blt $r4, $r0, .Lfail
    move $r24, $r4
    li.w $r11, 63
    move $r4, $r24
    move $r5, $r3
    li.w $r6, 2
    syscall 0
    li.w $r12, 2
    bne $r4, $r12, .Lfail
    ld.b $r12, $r3, 0
    li.w $r13, 104
    bne $r12, $r13, .Lfail
    li.w $r11, 62
    move $r4, $r23
    move $r5, $r0
    li.w $r6, 1
    syscall 0
    li.w $r12, 2
    bne $r4, $r12, .Lfail

    # pwrite 改文件内容，但不改变 dup 和原 FD 共享的当前游标。
    li.w $r11, 68
    move $r4, $r23
    la.local $r5, .Lxy
    li.w $r6, 2
    li.w $r7, 1
    syscall 0
    li.w $r12, 2
    bne $r4, $r12, .Lfail
    li.w $r11, 62
    move $r4, $r24
    move $r5, $r0
    li.w $r6, 1
    syscall 0
    li.w $r12, 2
    bne $r4, $r12, .Lfail

    # 从原 FD 归零，再通过副本 write；原 FD 应读到 "ZZ" 且游标为 2。
    li.w $r11, 62
    move $r4, $r23
    move $r5, $r0
    move $r6, $r0
    syscall 0
    bnez $r4, .Lfail
    li.w $r11, 64
    move $r4, $r24
    la.local $r5, .Lzz
    li.w $r6, 2
    syscall 0
    li.w $r12, 2
    bne $r4, $r12, .Lfail
    li.w $r11, 67
    move $r4, $r23
    move $r5, $r3
    li.w $r6, 3
    move $r7, $r0
    syscall 0
    li.w $r12, 3
    bne $r4, $r12, .Lfail
    ld.b $r12, $r3, 0
    li.w $r13, 90
    bne $r12, $r13, .Lfail
    ld.b $r12, $r3, 1
    bne $r12, $r13, .Lfail
    ld.b $r12, $r3, 2
    li.w $r13, 89
    bne $r12, $r13, .Lfail
    li.w $r11, 62
    move $r4, $r23
    move $r5, $r0
    li.w $r6, 1
    syscall 0
    li.w $r12, 2
    bne $r4, $r12, .Lfail

    # 两个 FD 分别关闭；只有全部检查通过才打印成功标记。
    li.w $r11, 57
    move $r4, $r24
    syscall 0
    bnez $r4, .Lfail
    li.w $r11, 57
    move $r4, $r23
    syscall 0
    bnez $r4, .Lfail
    li.w $r11, 64
    li.w $r4, 1
    la.local $r5, .Lmessage
    li.w $r6, 15
    syscall 0
    li.w $r12, 15
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
.Lpath:
    .asciz "/hello.txt"
.Lxy:
    .ascii "XY"
.Lzz:
    .ascii "ZZ"
.Lmessage:
    .ascii "LA_FD_OPS_PASS\n"
"#
);

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
