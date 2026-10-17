#![no_std]
#![no_main]

//! LoongArch M2.8 压力用例：连续 fork/wait4 128 个子任务并检查 COW 隔离。

use core::arch::global_asm;

global_asm!(
    r#"
    .section .text.entry, "ax"
    .globl _start
_start:
    # 状态和 canary 放在用户栈上；每轮子进程写同一虚拟地址触发 COW。
    addi.d $r3, $r3, -16
    st.w $r0, $r3, 0
    li.w $r12, 17
    st.w $r12, $r3, 4
    li.w $r20, 128
.Lround:
    # 父进程每轮创建一个子进程，避免同时积累大量僵尸任务。
    li.w $r11, 220
    syscall 0
    beqz $r4, .Lchild
    move $r23, $r4

    # 等待当前子进程，并验证 wait status 与父进程栈隔离。
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
    ld.w $r12, $r3, 4
    li.w $r13, 17
    bne $r12, $r13, .Lfail
    addi.w $r20, $r20, -1
    bnez $r20, .Lround

    # 唯一通过标记只在 128 次子进程全部回收后输出。
    li.w $r11, 64
    li.w $r4, 1
    la $r5, .Lmessage
    li.w $r6, 19
    syscall 0
    li.w $r12, 19
    bne $r4, $r12, .Lfail
    li.w $r4, 0
    b .Lexit
.Lchild:
    # 写共享虚拟地址；父进程稍后必须仍读到 17。
    li.w $r12, 34
    st.w $r12, $r3, 4
    ld.w $r13, $r3, 4
    bne $r12, $r13, .Lfail
    li.w $r4, 7
    b .Lexit
.Lfail:
    li.w $r4, 1
.Lexit:
    # 子任务退出后由父任务 wait4 回收地址空间和内核栈。
    li.w $r11, 93
    syscall 0
.Lpark:
    b .Lpark

    .section .rodata
.Lmessage:
    .ascii "LA_COW_STRESS_PASS\n"
"#
);

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
