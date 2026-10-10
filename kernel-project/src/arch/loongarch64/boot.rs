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

    # 临时桥（M0）：与 riscv64/boot.rs 对称，只有 core 0 进入 Rust。
    # 保留原因：M0 没有 per-hart 栈与 IPI，无法安全启动 secondary core。
    # 移除条件：M1 落地 arch::setup_all_cores 后改为 secondary 启动路径。
    bnez  $r12, .Lpark

    la    $r13, __boot_stack_top
    addi.d $r3, $r13, 0          # sp = $r3

    la    $r14, __bss_start
    la    $r15, __bss_end
.Lclear_bss:
    bgeu  $r14, $r15, .Lbss_done
    st.d  $r0, $r14, 0
    addi.d $r14, $r14, 8
    b     .Lclear_bss
.Lbss_done:
    addi.d $r4, $r12, 0          # a0 = hartid
    addi.d $r5, $r0, 0           # a1 = dtb_pa；LA 入口未提供，语义为「未提供」
    bl    kernel_main

.Lpark:
    idle  0
    b     .Lpark
"#
);
