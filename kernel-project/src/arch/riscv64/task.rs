//! 内核态上下文切换（W3 最小：callee-saved 现场 + __switch）。
//!
//! `TaskContext` 只保存内核态切换现场（ra/sp/s0-s11）；用户态现场由
//! `TrapContext`（内核栈顶下方）负责。布局与 `__riscv64_switch` 汇编共享
//! ABI，字段顺序禁止单独调整。

use core::arch::global_asm;

/// RISC-V 首任务沿用原有一页用户栈。
pub const INITIAL_USER_STACK_BYTES: usize = 0x1000;
/// RISC-V 用户栈顶保持在内核身份映射区下方。
pub const INITIAL_USER_STACK_TOP: usize = 0x7f00_0000;

/// 为首任务安装 RISC-V 定时器和外部中断路由。
pub fn prepare_first_task(hartid: usize) {
    super::timer::init();
    super::interrupt::init(hartid);
}

/// 按构建测试模式选择 RISC-V 首任务 ELF。
pub fn initial_user_image() -> &'static [u8] {
    #[cfg(feature = "uapp-fault-test")]
    const UAPP_ELF: &[u8] = include_bytes!("../../../build/user/uapp_fault.elf");
    // shell、COW、console、VFS 模式互斥选择一个首任务。
    #[cfg(all(
        feature = "uapp-shell",
        not(feature = "uapp-fault-test"),
        not(feature = "uapp-cow"),
        not(feature = "uapp-vfs"),
        not(feature = "uapp-console")
    ))]
    const UAPP_ELF: &[u8] = include_bytes!("../../../build/user/shell.elf");
    #[cfg(all(
        feature = "uapp-cow",
        not(feature = "uapp-shell"),
        not(feature = "uapp-fault-test"),
        not(feature = "uapp-vfs"),
        not(feature = "uapp-console")
    ))]
    const UAPP_ELF: &[u8] = include_bytes!("../../../build/user/uapp-cow.elf");
    #[cfg(all(
        feature = "uapp-console",
        not(feature = "uapp-vfs"),
        not(feature = "uapp-shell"),
        not(feature = "uapp-cow"),
        not(feature = "uapp-fault-test")
    ))]
    const UAPP_ELF: &[u8] = include_bytes!("../../../build/user/uapp-console.elf");
    #[cfg(all(feature = "uapp-vfs", not(feature = "uapp-fault-test")))]
    const UAPP_ELF: &[u8] = include_bytes!("../../../build/user/uapp-vfs.elf");
    // master 新增的 brk、pipe、mmap 定向程序仍由 RISC-V 架构入口选择。
    #[cfg(feature = "uapp-brk")]
    const UAPP_ELF: &[u8] = include_bytes!("../../../build/user/uapp-brk.elf");
    #[cfg(feature = "uapp-pipe")]
    const UAPP_ELF: &[u8] = include_bytes!("../../../build/user/uapp-pipe.elf");
    #[cfg(feature = "uapp-mmap")]
    const UAPP_ELF: &[u8] = include_bytes!("../../../build/user/uapp-mmap.elf");
    #[cfg(all(
        not(feature = "uapp-shell"),
        not(feature = "uapp-fault-test"),
        not(feature = "uapp-cow"),
        not(feature = "uapp-vfs"),
        not(feature = "uapp-console"),
        not(feature = "uapp-brk"),
        not(feature = "uapp-pipe"),
        not(feature = "uapp-mmap")
    ))]
    const UAPP_ELF: &[u8] = include_bytes!("../../../build/user/uapp.elf");
    UAPP_ELF
}

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

global_asm!(
    r#"
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
"#
);

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
