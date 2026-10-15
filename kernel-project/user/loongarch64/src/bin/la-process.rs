#![no_std]
#![no_main]

//! LoongArch 进程闭环：父 fork/wait4，子 exit(7)，父核对状态后输出标记。

use core::arch::global_asm;

global_asm!(
    r#"
    .section .text.entry, "ax"
    .globl _start
_start:
    # fork 前在用户栈留下共享初值；父进程稍后核对隔离效果。
    addi.d $r3, $r3, -16
    st.w $r0, $r3, 0
    li.w $r12, 17
    st.w $r12, $r3, 4
    # fork 返回：子进程 a0=0，父进程 a0=子 pid。
    li.w $r11, 220
    syscall 0
    beqz $r4, .Lchild
    move $r23, $r4

    # 父进程复用用户栈中的 Linux int 宽度 wait status 槽。
    li.w $r11, 260
    move $r4, $r23
    move $r5, $r3
    li.w $r6, 0
    li.w $r7, 0
    syscall 0
    bne $r4, $r23, .Lfail
    ld.w $r12, $r3, 0
    li.w $r13, 1792
    bne $r12, $r13, .Lfail
    # 子任务写过同一虚拟地址，父任务仍须看见 fork 前的值。
    ld.w $r12, $r3, 4
    li.w $r13, 17
    bne $r12, $r13, .Lfail

    # 只有父进程确认子 pid 和 exit(7) 状态后才发出通过标记。
    li.w $r11, 64
    li.w $r4, 1
    la $r5, .Lmessage
    li.w $r6, 16
    syscall 0
    li.w $r12, 16
    bne $r4, $r12, .Lfail
    li.w $r4, 0
    b .Lexit
.Lchild:
    # 子任务写用户栈触发用户态 COW；不能污染父任务的同一虚拟地址。
    li.w $r12, 34
    st.w $r12, $r3, 4
    ld.w $r13, $r3, 4
    bne $r12, $r13, .Lfail
    li.w $r4, 7
    b .Lexit
.Lfail:
    li.w $r4, 1
.Lexit:
    # exit 不应返回；若内核错误返回则只在用户态停驻。
    li.w $r11, 93
    syscall 0
.Lpark:
    b .Lpark

    .section .rodata
.Lmessage:
    .ascii "LA_PROCESS_PASS\n"
"#
);

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
