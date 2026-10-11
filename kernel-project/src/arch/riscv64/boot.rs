//! RISC-V64 早期启动汇编。
//!
//! 运行前提（QEMU virt + OpenSBI）：
//! - OpenSBI 以 S-mode 跳转到 ENTRY，a0 = hartid，a1 = DTB 物理地址；
//! - 未启用分页，地址即物理地址；
//! - 内核链接在 0x80200000（linker/riscv64.ld）。

use core::arch::global_asm;

global_asm!(
    r#"
    .section .text.entry
    .globl _start

_start:
    # M0 临时桥：每个核取自己的 boot 栈，全部进入 Rust，
    # 由 kernel_main 里的原子锁选主（见 kernel/mod.rs）。
    # 为什么不在汇编里选主：LLVM 对 global_asm 内联汇编的 Zaamo 指令检查
    # 与 Rust 生成代码不一致（实测 amoswap 汇编被拒、AtomicBool 可编译）。
    # 行为边界：hartid >= BOOT_HART_COUNT 的核不支持，直接停机。
    # 移除条件：M1 arch::setup_all_cores 落地后并入该路径。
    li t1, 8                    # BOOT_HART_COUNT（与 linker/riscv64.ld 一致）
    bgeu a0, t1, .Lpark
    la t2, __boot_stack_bottom
    # 每核栈顶 = bottom + (hartid+1)*8192。用移位代替乘法（8K = 2^13）：
    # LLVM 对 global_asm 只启用基础指令集，mul（Zmmul）/amo（Zaamo）都会被拒，
    # 即使 target 实际支持（与 Rust 生成代码的检查不一致）。
    slli t4, a0, 13             # t4 = hartid * 8192
    add t5, t2, t4
    li t3, 8192                 # BOOT_STACK_SIZE（每核 8K）
    add sp, t5, t3              # sp = bottom + (hartid+1)*size = 该核栈顶

    # 清 BSS。幂等：多个核先后执行只写 0，无副作用；
    # 每核的 sp 已独立，清零不会覆盖任何正在使用的栈。
    la t0, __bss_start
    la t1, __bss_end
.Lclear_bss:
    bgeu t0, t1, .Lbss_done
    sd zero, 0(t0)
    addi t0, t0, 8
    j .Lclear_bss
.Lbss_done:
    # a0 = hartid、a1 = DTB 物理地址，由 OpenSBI 传入，此处不再搬动。
    call kernel_main

    # kernel_main 声明为 -> !，返回即视为致命错误，落到停机而不是继续执行垃圾地址。
.Lpark:
    wfi
    j .Lpark
"#
);
