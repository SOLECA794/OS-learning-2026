#![no_std]
#![no_main]

//! 父进程永远不主动让出 CPU；子进程能退出才说明时钟触发了调度。

use core::arch::global_asm;

global_asm!(
    r#"
    .section .text.entry, "ax"
    .globl _start
_start:
    li.w $r11, 220
    syscall 0
    beqz $r4, .Lchild
    blt $r4, $r0, .Lfail

    # 父进程无系统调用、无等待、无显式调度；只有异步时钟能运行子进程。
.Lbusy:
    b .Lbusy

.Lchild:
    li.w $r11, 64
    li.w $r4, 1
    la.local $r5, .Lmessage
    li.w $r6, 16
    syscall 0
    li.w $r12, 16
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
    .ascii "LA_PREEMPT_PASS\n"
"#
);

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
