#![no_std]
#![no_main]

//! LoongArch M1.5 最小用户程序：write 成功后 exit(0)，否则 exit(1)。

use core::arch::global_asm;

global_asm!(
    r#"
    .section .text.entry, "ax"
    .globl _start
_start:
    # Linux write(1, message, 19)：必须经用户指针校验与控制台输出。
    li.w $r11, 64
    li.w $r4, 1
    la $r5, .Lmessage
    li.w $r6, 19
    syscall 0
    li.w $r12, 19
    bne $r4, $r12, .Lfail
    # 空指针必须返回 EFAULT，证明可返回 syscall 的现场恢复有效。
    li.w $r11, 64
    li.w $r4, 1
    li.w $r5, 0
    li.w $r6, 1
    syscall 0
    li.w $r12, -14
    bne $r4, $r12, .Lfail
    # 未知号必须返回 ENOSYS，而不是陷入内核 panic。
    li.w $r11, 999
    syscall 0
    li.w $r12, -38
    bne $r4, $r12, .Lfail
    li.w $r4, 0
    b .Lexit
.Lfail:
    li.w $r4, 1
.Lexit:
    # exit 成功后不再返回本程序；返回时只在用户态停驻。
    li.w $r11, 93
    syscall 0
.Lpark:
    b .Lpark

    .section .rodata
.Lmessage:
    .ascii "LA_USER_WRITE_PASS\n"
"#
);

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
