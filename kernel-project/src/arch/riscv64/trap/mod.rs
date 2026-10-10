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
