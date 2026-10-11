//! LoongArch64 trap 支持。
//!
//! `context` 定义 Rust 可访问的寄存器现场，`entry` 只保存汇编入口。

use core::arch::asm;

mod context;
mod entry;

pub use context::TrapContext;

/// 安装单入口模式的普通异常入口。
pub fn init() {
    unsafe extern "C" {
        fn __loongarch64_trap_entry();
    }
    let entry = __loongarch64_trap_entry as *const () as usize;
    debug_assert_eq!(entry & 0xfff, 0, "LoongArch EENTRY must be 4 KiB aligned");
    let vector_spacing_mask: usize = 0b111 << 16;
    // SAFETY: EENTRY 指向本模块 4 KiB 对齐的入口；清零 ECFG.VS 选择统一入口模式。
    unsafe {
        asm!(
            "csrwr {entry}, 0xc",
            "csrxchg $zero, {vs_mask}, 0x4",
            entry = inout(reg) entry => _,
            vs_mask = in(reg) vector_spacing_mask,
            options(nostack)
        );
    }
}
