//! LoongArch64 普通异常汇编保存/恢复路径。
//!
//! SAVE0 协议：用户态运行时为内核栈顶；内核态运行时为 0。
//! 入口只用 `$r3` 与 SAVE0 交换，避免在保存通用寄存器前覆盖用户现场。

use core::arch::global_asm;

use super::context::TRAP_CONTEXT_SIZE;

global_asm!(r#"
    .section .text.trap, "ax"
    .p2align 12
    .globl __loongarch64_trap_entry
__loongarch64_trap_entry:
    # 用户来源：r3 换成内核栈顶，SAVE0 暂存用户 sp。
    # 内核来源：SAVE0 为 0，立即交换回来，继续用原内核栈。
    csrwr $r3, 0x30
    beqz $r3, .Lkernel_origin
    addi.d $r3, $r3, -{frame_size}
    st.d $r12, $r3, 96
    csrrd $r12, 0x30
    st.d $r12, $r3, 24
    csrwr $r0, 0x30
    b .Lframe_ready
.Lkernel_origin:
    csrwr $r3, 0x30
    addi.d $r3, $r3, -{frame_size}
    st.d $r12, $r3, 96
    addi.d $r12, $r3, {frame_size}
    st.d $r12, $r3, 24
.Lframe_ready:
    st.d $r0, $r3, 0
    st.d $r1, $r3, 8
    st.d $r2, $r3, 16
    st.d $r4, $r3, 32
    st.d $r5, $r3, 40
    st.d $r6, $r3, 48
    st.d $r7, $r3, 56
    st.d $r8, $r3, 64
    st.d $r9, $r3, 72
    st.d $r10, $r3, 80
    st.d $r11, $r3, 88
    st.d $r13, $r3, 104
    st.d $r14, $r3, 112
    st.d $r15, $r3, 120
    st.d $r16, $r3, 128
    st.d $r17, $r3, 136
    st.d $r18, $r3, 144
    st.d $r19, $r3, 152
    st.d $r20, $r3, 160
    st.d $r21, $r3, 168
    st.d $r22, $r3, 176
    st.d $r23, $r3, 184
    st.d $r24, $r3, 192
    st.d $r25, $r3, 200
    st.d $r26, $r3, 208
    st.d $r27, $r3, 216
    st.d $r28, $r3, 224
    st.d $r29, $r3, 232
    st.d $r30, $r3, 240
    st.d $r31, $r3, 248
    csrrd $r12, 0x1
    st.d $r12, $r3, 256
    csrrd $r12, 0x6
    st.d $r12, $r3, 264
    csrrd $r12, 0x5
    st.d $r12, $r3, 272
    csrrd $r12, 0x7
    st.d $r12, $r3, 280
    addi.d $r4, $r3, 0
    bl kernel_trap_handler
    .globl __loongarch64_trap_restore
__loongarch64_trap_restore:
    # 返回用户态前重新发布内核栈顶；内核来源保持 SAVE0=0。
    ld.d $r12, $r3, 256
    andi $r12, $r12, 3
    beqz $r12, .Lrestore_regs
    addi.d $r12, $r3, {frame_size}
    csrwr $r12, 0x30
.Lrestore_regs:
    ld.d $r12, $r3, 256
    csrwr $r12, 0x1
    ld.d $r12, $r3, 264
    csrwr $r12, 0x6
    ld.d $r1, $r3, 8
    ld.d $r2, $r3, 16
    ld.d $r4, $r3, 32
    ld.d $r5, $r3, 40
    ld.d $r6, $r3, 48
    ld.d $r7, $r3, 56
    ld.d $r8, $r3, 64
    ld.d $r9, $r3, 72
    ld.d $r10, $r3, 80
    ld.d $r11, $r3, 88
    ld.d $r12, $r3, 96
    ld.d $r13, $r3, 104
    ld.d $r14, $r3, 112
    ld.d $r15, $r3, 120
    ld.d $r16, $r3, 128
    ld.d $r17, $r3, 136
    ld.d $r18, $r3, 144
    ld.d $r19, $r3, 152
    ld.d $r20, $r3, 160
    ld.d $r21, $r3, 168
    ld.d $r22, $r3, 176
    ld.d $r23, $r3, 184
    ld.d $r24, $r3, 192
    ld.d $r25, $r3, 200
    ld.d $r26, $r3, 208
    ld.d $r27, $r3, 216
    ld.d $r28, $r3, 224
    ld.d $r29, $r3, 232
    ld.d $r30, $r3, 240
    ld.d $r31, $r3, 248
    ld.d $r3, $r3, 24
    ertn
"#, frame_size = const TRAP_CONTEXT_SIZE);
