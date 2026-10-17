#![no_std]
#![no_main]

//! LoongArch 用户内存 syscall：验证 brk 堆页与匿名 mmap/munmap。

use core::arch::global_asm;

global_asm!(
    r#"
    .section .text.entry, "ax"
    .globl _start
_start:
    li.w $r21, 1
    # mmap 长度为零按 Linux ABI 返回 EINVAL。
    li.w $r11, 222
    move $r4, $r0
    move $r5, $r0
    syscall 0
    li.w $r12, -22
    bne $r4, $r12, .Lfail

    # brk(0) 返回共享 MemorySet 的初始堆顶 0x2000_0000。
    li.w $r11, 214
    move $r4, $r0
    syscall 0
    lu12i.w $r12, 0x20000
    bne $r4, $r12, .Lfail
    move $r23, $r4

    # 扩展两个页，再验证第二页写入可读。
    li.w $r11, 214
    lu12i.w $r4, 0x20002
    move $r12, $r4
    syscall 0
    bne $r4, $r12, .Lfail
    li.d $r16, 4096
    add.d $r13, $r23, $r16
    li.w $r14, 0x5a
    st.b $r14, $r13, 0
    ld.b $r15, $r13, 0
    bne $r14, $r15, .Lfail

    # 收缩到首个堆页，保留页仍可读写。
    li.w $r11, 214
    add.d $r12, $r23, $r16
    move $r4, $r12
    syscall 0
    bne $r4, $r12, .Lfail
    li.w $r14, 0x33
    st.b $r14, $r23, 0
    ld.b $r15, $r23, 0
    bne $r14, $r15, .Lfail

    li.w $r21, 2
    # mmap 三页，后续只解除中间一页以检查区域拆分与邻页保留。
    li.w $r11, 222
    move $r4, $r0
    li.w $r5, 12288
    li.w $r6, 3
    li.w $r7, 0x22
    li.w $r8, -1
    move $r9, $r0
    syscall 0
    blt $r4, $r0, .Lfail
    move $r24, $r4
    li.w $r21, 3
    li.d $r17, 8192
    li.w $r14, 0x6b
    st.b $r14, $r24, 0
    ld.b $r15, $r24, 0
    bne $r14, $r15, .Lfail
    li.w $r14, 0x7c
    add.d $r13, $r24, $r17
    st.b $r14, $r13, 0
    ld.b $r15, $r13, 0
    bne $r14, $r15, .Lfail

    li.w $r21, 4
    # Linux munmap 要求地址页对齐且长度非零；两种非法参数都返回 EINVAL。
    li.w $r21, 41
    li.w $r11, 215
    addi.d $r4, $r24, 1
    li.w $r5, 4096
    syscall 0
    li.w $r12, -22
    beq $r4, $r12, .Lbad_address_ok
    move $r21, $r4
    b .Lfail
.Lbad_address_ok:
    li.w $r21, 42
    li.w $r11, 215
    move $r4, $r24
    move $r5, $r0
    syscall 0
    li.w $r12, -22
    beq $r4, $r12, .Lbad_length_ok
    move $r21, $r4
    b .Lfail
.Lbad_length_ok:

    li.w $r21, 5
    # 仅解除中间页；两侧映射仍须有效，洞上的 copy_from_user 应返回 EFAULT。
    li.w $r11, 215
    add.d $r4, $r24, $r16
    li.w $r5, 4096
    syscall 0
    bnez $r4, .Lfail
    li.w $r21, 6
    li.w $r14, 0x6b
    ld.b $r15, $r24, 0
    bne $r14, $r15, .Lfail
    li.w $r14, 0x7c
    add.d $r13, $r24, $r17
    ld.b $r15, $r13, 0
    bne $r14, $r15, .Lfail
    li.w $r11, 64
    li.w $r4, 1
    add.d $r5, $r24, $r16
    li.w $r6, 1
    syscall 0
    li.w $r12, -14
    bne $r4, $r12, .Lfail

    li.w $r21, 7
    # 范围可包含已解除的洞；剩余两页应全部释放。
    li.w $r11, 215
    move $r4, $r24
    li.w $r5, 12288
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
    move $r4, $r21
.Lexit:
    li.w $r11, 93
    syscall 0
.Lpark:
    b .Lpark

    .section .rodata
.Lmessage:
    .ascii "LA_MEMORY_PASS\n"
"#
);

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
