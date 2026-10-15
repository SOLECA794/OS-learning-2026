#![no_std]
#![no_main]

//! LoongArch 目录与 FD 第三批验收：创建、枚举、stat 和 dup2。

use core::arch::global_asm;

global_asm!(
    r#"
    .section .text.entry, "ax"
    .globl _start
_start:
    # 栈中前 128 字节放 stat，后 32 字节放 dirent，避免缓冲互相覆盖。
    addi.d $r3, $r3, -256
    li.w $r11, 34
    li.w $r4, -100
    la.local $r5, .Ldir
    li.w $r6, 493
    syscall 0
    bnez $r4, .Lfail

    # O_DIRECTORY 打开新目录，然后用目录 FD 相对创建文件。
    li.w $r11, 56
    li.w $r4, -100
    la.local $r5, .Ldir
    li.w $r6, 0x10000
    move $r7, $r0
    syscall 0
    blt $r4, $r0, .Lfail
    move $r23, $r4
    li.w $r11, 56
    move $r4, $r23
    la.local $r5, .Litem
    li.w $r6, 0x42
    li.w $r7, 420
    syscall 0
    blt $r4, $r0, .Lfail
    move $r24, $r4

    # dup2 指向同一打开描述；关闭旧 FD 后新 FD 仍可写。
    li.w $r11, 24
    move $r4, $r24
    li.w $r5, 9
    syscall 0
    li.w $r12, 9
    bne $r4, $r12, .Lfail
    li.w $r11, 57
    move $r4, $r24
    syscall 0
    bnez $r4, .Lfail
    li.w $r11, 64
    li.w $r4, 9
    la.local $r5, .Ldata
    li.w $r6, 4
    syscall 0
    li.w $r12, 4
    bne $r4, $r12, .Lfail

    # fstat 的 mode、size 按公共 128 字节 UAPI 写回。
    li.w $r11, 80
    li.w $r4, 9
    move $r5, $r3
    syscall 0
    bnez $r4, .Lfail
    ld.w $r12, $r3, 16
    li.w $r13, 32768
    bne $r12, $r13, .Lfail
    ld.d $r12, $r3, 48
    li.w $r13, 4
    bne $r12, $r13, .Lfail

    # 枚举唯一目录项，核对记录长度、类型和文件名。
    li.w $r11, 217
    move $r4, $r23
    addi.d $r5, $r3, 160
    li.w $r6, 32
    syscall 0
    li.w $r12, 24
    bne $r4, $r12, .Lfail
    ld.hu $r12, $r3, 176
    li.w $r13, 24
    bne $r12, $r13, .Lfail
    ld.b $r12, $r3, 178
    li.w $r13, 8
    bne $r12, $r13, .Lfail
    ld.b $r12, $r3, 179
    li.w $r13, 105
    bne $r12, $r13, .Lfail
    ld.b $r12, $r3, 183
    bnez $r12, .Lfail
    # 第二次枚举应为 EOF，不能重复返回同一目录项。
    li.w $r11, 217
    move $r4, $r23
    addi.d $r5, $r3, 160
    li.w $r6, 32
    syscall 0
    bnez $r4, .Lfail

    # 非法 FD 与非目录边界分别返回 EBADF/ENOTDIR。
    li.w $r11, 80
    li.w $r4, 99
    move $r5, $r3
    syscall 0
    li.w $r12, -9
    bne $r4, $r12, .Lfail
    li.w $r11, 217
    li.w $r4, 9
    addi.d $r5, $r3, 160
    li.w $r6, 32
    syscall 0
    li.w $r12, -20
    bne $r4, $r12, .Lfail

    li.w $r11, 57
    li.w $r4, 9
    syscall 0
    bnez $r4, .Lfail
    li.w $r11, 57
    move $r4, $r23
    syscall 0
    bnez $r4, .Lfail
    li.w $r11, 64
    li.w $r4, 1
    la.local $r5, .Lmessage
    li.w $r6, 12
    syscall 0
    li.w $r12, 12
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
.Ldir:
    .asciz "/la-dir"
.Litem:
    .asciz "item"
.Ldata:
    .ascii "data"
.Lmessage:
    .ascii "LA_DIR_PASS\n"
"#
);

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
