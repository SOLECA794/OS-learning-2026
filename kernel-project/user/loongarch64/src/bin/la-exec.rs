#![no_std]
#![no_main]

//! LoongArch execve 验收：先验证失败不损坏旧现场，再替换为目标 ELF。

use core::arch::global_asm;

global_asm!(
    r#"
    .section .text.entry, "ax"
    .globl _start
_start:
    # 不存在的路径应返回 ENOENT(-2)，旧程序继续执行。
    li.w $r11, 221
    la.local $r4, .Lmissing
    move $r5, $r0
    move $r6, $r0
    syscall 0
    li.w $r12, -2
    bne $r4, $r12, .Lbad_errno

    # argv 首项为 "check"、末项为 NULL；目标程序检查 argc 和字符串。
    li.w $r11, 221
    la.local $r4, .Ltarget
    la.local $r5, .Largv
    move $r6, $r0
    syscall 0
    # 成功的 exec 不应返回旧入口；3 表示调用返回了旧现场。
    li.w $r4, 3
    b .Lexit
.Lbad_errno:
    # 2 表示不存在路径的错误码不符。
    li.w $r4, 2
.Lexit:
    li.w $r11, 93
    syscall 0
.Lpark:
    b .Lpark

    .section .rodata
.Lmissing:
    .asciz "/la-no-such-file"
.Ltarget:
    .asciz "/la-exec-target"
.Larg:
    .asciz "check"
    .balign 8
.Largv:
    .dword .Larg
    .dword 0
"#
);

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
