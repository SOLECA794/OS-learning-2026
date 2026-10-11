//! LoongArch64 早期启动汇编。
//!
//! 运行前提（QEMU virt，`-kernel` 直接装载 ELF）：
//! - 入口处 CRMD.PG=0 / DA=1：物理直址，分页尚未建立；
//! - 内核链接在 0x00200000，位于低 256MB RAM 段内（linker/loongarch64.ld）；
//! - 与 RISC-V 不同，`-kernel` 入口**不通过寄存器传递 DTB 地址**，因此 a1 传 0。
//!
//! 寄存器一律使用 `$rN` 编号写法：编号名在两个汇编器上都可用，ABI 名（$t0/$a0）不是。

use core::arch::global_asm;

global_asm!(
    r#"
    .section .text.entry
    .globl _start

_start:
    # CPUID（CSR 0x20）低 9 位是 core id。
    csrrd $r12, 0x20
    andi  $r12, $r12, 0x1ff

    # M0 临时桥：per-hart 栈 + Rust 侧原子选主（与 riscv64/boot.rs 对称）。
    # 为什么：QEMU -kernel 模式当前只引导主核，但评测机 / 实板可能引导全部核；
    # 每个核取自己的栈全部进入 Rust，由 kernel_main 原子锁选主。
    # 移除条件：M1 arch::setup_all_cores 落地后并入该路径。
    li.d  $r13, 8               # BOOT_HART_COUNT（与 linker/loongarch64.ld 一致）
    bgeu  $r12, $r13, .Lpark
    la    $r14, __boot_stack_bottom
    # 每核栈顶 = bottom + (core+1)*8192。用移位代替乘法（8K = 2^13）：
    # LLVM 对 global_asm 只启用基础指令集（与 Rust 生成代码的检查不一致）。
    slli.d $r17, $r12, 13       # r17 = core * 8192
    add.d $r18, $r14, $r17
    li.d  $r15, 8192            # BOOT_STACK_SIZE（每核 8K）
    add.d $r3, $r18, $r15       # sp = bottom + (core+1)*size = 该核栈顶

    # BSS 清零由主核在 kernel_main 内完成（先经 .data 的 BOOT_LOCK 选主再清，
    # 从核不清、不初始化，直接停驻；见 kernel/mod.rs 与 riscv64/boot.rs 说明）。
    addi.d $r4, $r12, 0          # a0 = hartid（core id）
    addi.d $r5, $r0, 0           # a1 = dtb_pa；LA 入口未提供，语义为「未提供」
    bl    kernel_main

.Lpark:
    idle  0
    b     .Lpark
"#
);
