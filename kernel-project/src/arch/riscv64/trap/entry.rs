//! RISC-V64 trap 汇编保存/恢复路径（M1.5：支持用户态/内核态来源切换）。
//!
//! sscratch 协议：用户态运行时 `sscratch = 当前任务内核栈顶`；内核态运行时
//! `sscratch = 0`。进入时 `csrrw sp, sscratch, sp` 交换：交换后 `sp != 0` 说明
//! 来自用户态（已切到内核栈顶），`sp == 0` 说明来自内核态（换回）。
//! 恢复段 `__riscv64_trap_restore` 同时服务 trap 返回与「从构造好的 TrapContext
//! 进入用户态」（初始化流程复用）。内核处理期间置 sstatus.SUM=1 以访问用户页，
//! 恢复时写回保存的 sstatus 自动清除。

use core::arch::global_asm;

use super::context::TRAP_CONTEXT_SIZE;

global_asm!(r#"
    .section .text.trap, "ax"
    .balign 4
    .globl __riscv64_trap_entry
__riscv64_trap_entry:
    csrrw sp, sscratch, sp
    bnez sp, 1f
    csrrw sp, sscratch, sp
1:
    addi sp, sp, -{frame_size}
    sd zero, 0(sp)
    sd ra, 8(sp)
    sd t0, 40(sp)
    addi t0, sp, {frame_size}
    sd t0, 16(sp)
    sd gp, 24(sp)
    sd tp, 32(sp)
    sd t1, 48(sp)
    sd t2, 56(sp)
    sd s0, 64(sp)
    sd s1, 72(sp)
    sd a0, 80(sp)
    sd a1, 88(sp)
    sd a2, 96(sp)
    sd a3, 104(sp)
    sd a4, 112(sp)
    sd a5, 120(sp)
    sd a6, 128(sp)
    sd a7, 136(sp)
    sd s2, 144(sp)
    sd s3, 152(sp)
    sd s4, 160(sp)
    sd s5, 168(sp)
    sd s6, 176(sp)
    sd s7, 184(sp)
    sd s8, 192(sp)
    sd s9, 200(sp)
    sd s10, 208(sp)
    sd s11, 216(sp)
    sd t3, 224(sp)
    sd t4, 232(sp)
    sd t5, 240(sp)
    sd t6, 248(sp)
    csrr t0, sstatus
    sd t0, 256(sp)
    csrr t0, sepc
    sd t0, 264(sp)
    csrr t0, scause
    sd t0, 272(sp)
    csrr t0, stval
    sd t0, 280(sp)
    # 来源判定：sstatus.SPP（1=内核态，0=用户态）。
    # 用户态：x2 槽 = 用户 sp（进入时暂存于 sscratch），并把 sscratch 清零。
    ld t0, 256(sp)
    srli t0, t0, 8
    andi t0, t0, 1
    bnez t0, 2f
    csrr t1, sscratch
    sd t1, 16(sp)
    csrw sscratch, zero
2:
    # 内核处理期间允许 S-mode 访问用户页（SUM=1）。
    li t0, 1 << 18
    csrs sstatus, t0
    mv a0, sp
    call kernel_trap_handler

    .globl __riscv64_trap_restore
__riscv64_trap_restore:
    # 来源判定复用现场保存的 sstatus.SPP：
    # 用户态返回 → sscratch 重置为内核栈顶（= 现场顶）；内核态返回 → 保持 0。
    ld t0, 256(sp)
    srli t0, t0, 8
    andi t0, t0, 1
    bnez t0, 3f
    addi t0, sp, {frame_size}
    csrw sscratch, t0
3:
    ld t0, 256(sp)
    csrw sstatus, t0
    ld t0, 264(sp)
    csrw sepc, t0
    ld ra, 8(sp)
    ld gp, 24(sp)
    ld tp, 32(sp)
    ld t0, 40(sp)
    ld t1, 48(sp)
    ld t2, 56(sp)
    ld s0, 64(sp)
    ld s1, 72(sp)
    ld a0, 80(sp)
    ld a1, 88(sp)
    ld a2, 96(sp)
    ld a3, 104(sp)
    ld a4, 112(sp)
    ld a5, 120(sp)
    ld a6, 128(sp)
    ld a7, 136(sp)
    ld s2, 144(sp)
    ld s3, 152(sp)
    ld s4, 160(sp)
    ld s5, 168(sp)
    ld s6, 176(sp)
    ld s7, 184(sp)
    ld s8, 192(sp)
    ld s9, 200(sp)
    ld s10, 208(sp)
    ld s11, 216(sp)
    ld t3, 224(sp)
    ld t4, 232(sp)
    ld t5, 240(sp)
    ld t6, 248(sp)
    ld sp, 16(sp)
    sret
"#, frame_size = const TRAP_CONTEXT_SIZE);
