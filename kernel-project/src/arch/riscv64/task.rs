//! 内核态上下文切换（W3 最小：callee-saved 现场 + __switch）。
//!
//! `TaskContext` 只保存内核态切换现场（ra/sp/s0-s11）；用户态现场由
//! `TrapContext`（内核栈顶下方）负责。布局与 `__riscv64_switch` 汇编共享
//! ABI，字段顺序禁止单独调整。

use core::arch::global_asm;

/// 内核态切换现场。`repr(C)` 保证布局与汇编存取一致。
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TaskContext {
    pub ra: usize,
    pub sp: usize,
    pub s0: usize,
    pub s1: usize,
    pub s2: usize,
    pub s3: usize,
    pub s4: usize,
    pub s5: usize,
    pub s6: usize,
    pub s7: usize,
    pub s8: usize,
    pub s9: usize,
    pub s10: usize,
    pub s11: usize,
}

impl TaskContext {
    pub const fn zero_init() -> Self {
        Self {
            ra: 0,
            sp: 0,
            s0: 0,
            s1: 0,
            s2: 0,
            s3: 0,
            s4: 0,
            s5: 0,
            s6: 0,
            s7: 0,
            s8: 0,
            s9: 0,
            s10: 0,
            s11: 0,
        }
    }
}

global_asm!(r#"
    .section .text.trap, "ax"
    .balign 4
    .globl __riscv64_switch
__riscv64_switch:
    sd ra, 0(a0)
    sd sp, 8(a0)
    sd s0, 16(a0)
    sd s1, 24(a0)
    sd s2, 32(a0)
    sd s3, 40(a0)
    sd s4, 48(a0)
    sd s5, 56(a0)
    sd s6, 64(a0)
    sd s7, 72(a0)
    sd s8, 80(a0)
    sd s9, 88(a0)
    sd s10, 96(a0)
    sd s11, 104(a0)
    ld ra, 0(a1)
    ld sp, 8(a1)
    ld s0, 16(a1)
    ld s1, 24(a1)
    ld s2, 32(a1)
    ld s3, 40(a1)
    ld s4, 48(a1)
    ld s5, 56(a1)
    ld s6, 64(a1)
    ld s7, 72(a1)
    ld s8, 80(a1)
    ld s9, 88(a1)
    ld s10, 96(a1)
    ld s11, 104(a1)
    ret
"#);

/// 保存当前现场到 `old`，恢复 `new` 现场并从 `new.ra` 继续执行。
///
/// 通过 C ABI 调用：参数落在 a0/a1，编译器生成的 call 返回地址即 `old.ra`
/// （被 `__riscv64_switch` 保存）；恢复后 `ret` 跳到 `new.ra`，不回到本函数。
pub fn switch(old: *mut TaskContext, new: *const TaskContext) {
    unsafe extern "C" {
        fn __riscv64_switch(old: usize, new: usize);
    }
    // SAFETY: 调用方保证 old/new 指向存活且互异的有效 TaskContext；
    // __riscv64_switch 仅读写这两帧（a0/a1 指定），恢复后跳到 new.ra。
    unsafe { __riscv64_switch(old as usize, new as usize) }
}

/// 首次进入用户态的恢复点地址（`__riscv64_trap_restore`），
/// 供新任务 `TaskContext.ra` 使用：`__switch` 恢复后先执行 `__restore`，
/// 用当前栈顶 TrapContext 进入（或返回）用户态。
pub fn trap_return_address() -> usize {
    unsafe extern "C" {
        fn __riscv64_trap_restore();
    }
    __riscv64_trap_restore as usize
}
