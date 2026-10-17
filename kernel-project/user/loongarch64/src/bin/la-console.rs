#![no_std]
#![no_main]

//! LoongArch QEMU virt 控制台验收：空读阻塞，UART RX IRQ 到达后读者被唤醒。

use core::arch::global_asm;

global_asm!(
    r#"
    .section .text.entry, "ax"
    .globl _start
_start:
    # 父任务等待子读者；无其它就绪任务后内核进入可中断 idle。
    addi.d $r3, $r3, -16
    st.w $r0, $r3, 0
    li.w $r11, 220
    syscall 0
    beqz $r4, .Lreader
    move $r23, $r4

    # 子读者退出 7 后父任务输出唯一成功标记。
    li.w $r11, 260
    move $r4, $r23
    move $r5, $r3
    li.w $r6, 0
    li.w $r7, 0
    syscall 0
    bne $r4, $r23, .Lwait_fail
    ld.w $r12, $r3, 0
    li.w $r13, 1792
    bne $r12, $r13, .Lstatus_fail
    li.w $r11, 64
    li.w $r4, 1
    la.local $r5, .Lpass
    li.w $r6, 16
    syscall 0
    li.w $r12, 16
    bne $r4, $r12, .Lwrite_fail
    li.w $r4, 0
    b .Lexit

.Lreader:
    # 运行器观察 idle 标记后才注入 fixture，确保 read 已登记等待。
    li.w $r11, 64
    li.w $r4, 1
    la.local $r5, .Lready
    li.w $r6, 22
    syscall 0
    li.w $r12, 22
    bne $r4, $r12, .Lfail
    li.w $r20, 0
.Lread_more:
    # 允许 UART 分批交付数据；循环累计短读直到获得 abc\n。
    li.w $r11, 63
    li.w $r4, 0
    addi.d $r5, $r3, 4
    add.d $r5, $r5, $r20
    li.w $r6, 4
    sub.w $r6, $r6, $r20
    syscall 0
    blez $r4, .Lfail
    add.w $r20, $r20, $r4
    li.w $r13, 4
    blt $r20, $r13, .Lread_more
    ld.w $r12, $r3, 4
    li.w $r13, 0x0a636261
    bne $r12, $r13, .Lfail
    li.w $r4, 7
    b .Lexit

.Lfail:
    li.w $r4, 1
    b .Lexit
.Lwait_fail:
    li.w $r4, 11
    b .Lreport_fail
.Lstatus_fail:
    li.w $r4, 12
    b .Lreport_fail
.Lwrite_fail:
    # 把 write 的实际返回值作为退出码留给串口日志定位。
    b .Lexit
.Lreport_fail:
    move $r20, $r4
    li.w $r11, 64
    li.w $r4, 1
    la.local $r5, .Lfailure
    li.w $r6, 16
    syscall 0
    move $r4, $r20
.Lexit:
    # exit 后控制权交给父任务或 runner 收尾，不再访问用户栈。
    li.w $r11, 93
    syscall 0
.Lpark:
    b .Lpark

    .section .rodata
.Lready:
    .ascii "LA_CONSOLE_WAIT_READY\n"
.Lpass:
    .ascii "LA_CONSOLE_PASS\n"
.Lfailure:
    .ascii "LA_CONSOLE_FAIL\n"
"#
);

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
