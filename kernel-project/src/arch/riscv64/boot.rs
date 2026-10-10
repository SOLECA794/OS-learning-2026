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
    # 临时桥（M0）：只有 bootstrap hart 进入 Rust，其余 hart 直接停机。
    # 保留原因：M0 没有 per-hart 栈、没有 IPI、没有调度器，无法安全启动 secondary hart。
    # 行为边界：secondary hart 在 wfi 上等待，不触碰 BSS、不触碰串口。
    # 移除条件：M1 落地 arch::setup_all_cores 后，此分支改为跳转到 secondary 启动路径。
    bnez a0, .Lpark

    la sp, __boot_stack_top

    # 清 BSS。sp 已指向 boot 栈顶且栈向下生长，清零不会覆盖正在使用的栈空间。
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
