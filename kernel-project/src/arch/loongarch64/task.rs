//! LoongArch64 内核态任务切换现场。
//!
//! 这里只保存调用约定要求被调用方保留的寄存器；用户寄存器由 `TrapContext` 保存。
//! 字段顺序与下方汇编偏移共同构成内部 ABI，修改时必须同步检查。

use core::arch::global_asm;

/// 临时用户栈布局：原 0x7f00_0000 在 QEMU 首次写栈时触发 PIS(2)，
/// 0x1001_0000 的定向 fork/COW 测试通过，且避开内核低 256 MiB 身份映射。
/// 行为边界：仅证明该低虚拟地址布局；修复高地址页表遍历后再恢复统一栈顶。
pub const INITIAL_USER_STACK_TOP: usize = 0x1001_0000;
/// 首任务先沿用一页用户栈，按硬件页表路径独立验收。
pub const INITIAL_USER_STACK_BYTES: usize = 0x1000;

/// 当前 LoongArch 构建的首个用户 ELF；与 RISC-V ecall 程序互不混用。
pub fn initial_user_image() -> &'static [u8] {
    // 进程验收模式使用独立 ELF，默认仍运行 M1.5 用户程序。
    #[cfg(feature = "la-fd-ops-test")]
    const UAPP_ELF: &[u8] = include_bytes!("../../../build/user/la-fd-ops.elf");
    #[cfg(all(not(feature = "la-fd-ops-test"), feature = "la-file-test"))]
    const UAPP_ELF: &[u8] = include_bytes!("../../../build/user/la-file.elf");
    #[cfg(all(
        not(feature = "la-fd-ops-test"),
        not(feature = "la-file-test"),
        feature = "la-exec-test"
    ))]
    const UAPP_ELF: &[u8] = include_bytes!("../../../build/user/la-exec.elf");
    #[cfg(all(
        not(feature = "la-fd-ops-test"),
        not(feature = "la-file-test"),
        not(feature = "la-exec-test"),
        feature = "la-process-test"
    ))]
    const UAPP_ELF: &[u8] = include_bytes!("../../../build/user/la-process.elf");
    #[cfg(not(any(
        feature = "la-fd-ops-test",
        feature = "la-file-test",
        feature = "la-exec-test",
        feature = "la-process-test"
    )))]
    const UAPP_ELF: &[u8] = include_bytes!("../../../build/user/la-uapp.elf");
    UAPP_ELF
}

/// LoongArch 尚未接入时钟中断，首任务仅以同步 trap 运行。
pub fn prepare_first_task(_hartid: usize) {}

/// 内核栈指针、返回地址以及 LoongArch ABI 的被调用方保存寄存器。
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TaskContext {
    pub ra: usize,
    pub sp: usize,
    pub fp: usize,
    pub s0: usize,
    pub s1: usize,
    pub s2: usize,
    pub s3: usize,
    pub s4: usize,
    pub s5: usize,
    pub s6: usize,
    pub s7: usize,
    pub s8: usize,
}

// 汇编按 12 个 8 字节槽访问现场；编译期阻止布局大小意外变化。
const _: () = assert!(core::mem::size_of::<TaskContext>() == 96);

impl TaskContext {
    /// 为首次调度的新任务建立空现场；调用方随后填入 `ra` 与 `sp`。
    pub const fn zero_init() -> Self {
        Self {
            ra: 0,
            sp: 0,
            fp: 0,
            s0: 0,
            s1: 0,
            s2: 0,
            s3: 0,
            s4: 0,
            s5: 0,
            s6: 0,
            s7: 0,
            s8: 0,
        }
    }
}

global_asm!(
    r#"
    .section .text.trap, "ax"
    .balign 4
    .globl __loongarch64_switch
__loongarch64_switch:
    # a0/r4 = old，a1/r5 = new；保存当前内核调用现场。
    st.d $r1, $r4, 0
    st.d $r3, $r4, 8
    st.d $r22, $r4, 16
    st.d $r23, $r4, 24
    st.d $r24, $r4, 32
    st.d $r25, $r4, 40
    st.d $r26, $r4, 48
    st.d $r27, $r4, 56
    st.d $r28, $r4, 64
    st.d $r29, $r4, 72
    st.d $r30, $r4, 80
    st.d $r31, $r4, 88
    # 恢复目标任务；返回地址可能是其首次运行的 trap 恢复入口。
    ld.d $r1, $r5, 0
    ld.d $r3, $r5, 8
    ld.d $r22, $r5, 16
    ld.d $r23, $r5, 24
    ld.d $r24, $r5, 32
    ld.d $r25, $r5, 40
    ld.d $r26, $r5, 48
    ld.d $r27, $r5, 56
    ld.d $r28, $r5, 64
    ld.d $r29, $r5, 72
    ld.d $r30, $r5, 80
    ld.d $r31, $r5, 88
    jirl $r0, $r1, 0
"#
);

/// 保存 `old` 并从 `new` 的保存点继续执行。
pub fn switch(old: *mut TaskContext, new: *const TaskContext) {
    unsafe extern "C" {
        fn __loongarch64_switch(old: *mut TaskContext, new: *const TaskContext);
    }
    // SAFETY: 调用方保证现场指针有效且互异；汇编仅访问两块固定大小的现场。
    unsafe { __loongarch64_switch(old, new) }
}

/// 新任务首次运行时，从内核栈上的 trap 现场进入用户态。
pub fn trap_return_address() -> usize {
    unsafe extern "C" {
        fn __loongarch64_trap_restore();
    }
    __loongarch64_trap_restore as usize
}

/// 仅测试构建使用的跨内核栈往返切换。
#[cfg(feature = "la-switch-test")]
pub fn self_test() {
    use core::sync::atomic::Ordering;

    // B 栈独立分配；两个现场在 A 栈上保持存活直到切换返回。
    let b_stack = crate::kernel::mm::kernel_stack::KernelStack::new()
        .expect("LA switch test stack allocation failed");
    let mut a = TaskContext::zero_init();
    let mut b = TaskContext {
        ra: switch_test_entry as usize,
        sp: b_stack.top(),
        ..TaskContext::zero_init()
    };
    TEST_A.store(&mut a as *mut _ as usize, Ordering::Release);
    TEST_B.store(&mut b as *mut _ as usize, Ordering::Release);
    TEST_PHASE.store(0, Ordering::Release);
    switch(&mut a, &b);
    assert_eq!(TEST_PHASE.load(Ordering::Acquire), 1);
    TEST_A.store(0, Ordering::Release);
    TEST_B.store(0, Ordering::Release);
    crate::log_info!("LA_SWITCH_PASS");
}

// 测试只在单核引导期间运行；原子槽负责把 A 栈中的现场地址交给 B 栈。
#[cfg(feature = "la-switch-test")]
static TEST_A: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);
#[cfg(feature = "la-switch-test")]
static TEST_B: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);
#[cfg(feature = "la-switch-test")]
static TEST_PHASE: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

/// B 栈首次执行点：记录到达后切回 A 栈，绝不能自然返回。
#[cfg(feature = "la-switch-test")]
extern "C" fn switch_test_entry() -> ! {
    use core::sync::atomic::Ordering;

    let a = TEST_A.load(Ordering::Acquire) as *const TaskContext;
    let b = TEST_B.load(Ordering::Acquire) as *mut TaskContext;
    assert!(!a.is_null() && !b.is_null());
    TEST_PHASE.store(1, Ordering::Release);
    switch(b, a);
    panic!("LA switch test unexpectedly resumed B stack");
}
