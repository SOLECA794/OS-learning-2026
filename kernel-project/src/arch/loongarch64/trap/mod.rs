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

/// SAVE0 协议：用户态运行时保存当前任务的内核栈顶，内核态运行时为零。
pub fn set_user_scratch(value: usize) {
    // SAFETY: CSRWR 的输入寄存器会被旧 CSR 值覆盖，故使用 inout 约束。
    unsafe {
        asm!("csrwr {value}, 0x30", value = inout(reg) value => _, options(nostack));
    }
}

/// 从内核栈上的现场经 ERTN 进入或返回用户态。
pub fn restore_from(context: *const TrapContext) -> ! {
    unsafe extern "C" {
        fn __loongarch64_trap_restore();
    }
    // SAFETY: context 是有效内核栈上的完整现场；汇编恢复现场后不返回。
    unsafe {
        asm!(
            "move $r3, {context}",
            "jirl $r0, {restore}, 0",
            context = in(reg) context as usize,
            restore = in(reg) __loongarch64_trap_restore as usize,
            options(noreturn, nostack)
        );
    }
}
