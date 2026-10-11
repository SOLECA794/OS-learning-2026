//! RISC-V64 trap 支持。
//!
//! `context` 定义 Rust 可访问的寄存器现场，`entry` 只保存汇编入口。

use core::arch::asm;

mod context;
mod entry;

pub use context::TrapContext;

/// 安装 Direct 模式的 S-mode trap 入口。
pub fn init() {
    unsafe extern "C" {
        fn __riscv64_trap_entry();
    }
    let entry = __riscv64_trap_entry as *const () as usize;
    debug_assert_eq!(entry & 0b11, 0, "RISC-V stvec must be 4-byte aligned");
    // SAFETY: 入口地址有效且对齐；这里只安装入口，不开启异步中断。
    unsafe {
        asm!(
            "csrw stvec, {entry}",
            "csrw sscratch, zero",
            entry = in(reg) entry,
            options(nostack, preserves_flags)
        );
    }
}

/// 设置 sscratch（M1.5 用户态切换协议：用户态运行时 = 当前任务内核栈顶）。
pub fn set_user_scratch(value: usize) {
    // SAFETY: 写架构 CSR，无前置条件。
    unsafe {
        asm!(
            "csrw sscratch, {value}",
            value = in(reg) value,
            options(nostack, preserves_flags)
        );
    }
}

/// 从构造好的 TrapContext 进入（或返回）用户态。调用后不返回。
///
/// `context` 指向一帧完整的 TrapContext（恢复段会按 SPP 决定是否重置 sscratch）。
pub fn restore_from(context: *const TrapContext) -> ! {
    unsafe extern "C" {
        fn __riscv64_trap_restore();
    }
    let restore = __riscv64_trap_restore as usize;
    // SAFETY: context 指向有效现场；jr 不返回，选项声明 noreturn。
    unsafe {
        asm!(
            "mv sp, {ctx}",
            "jr {restore}",
            ctx = in(reg) context as usize,
            restore = in(reg) restore,
            options(noreturn, nostack)
        );
    }
}
