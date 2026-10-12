//! RISC-V64 后端。所有 RISC-V 特有的寄存器/CSR/汇编只允许出现在本目录内。

pub mod boot;
pub mod console;
pub mod mm;
pub mod power;
pub mod task;
pub mod timer;
pub mod trap;

pub use power::{power_off, restart};

/// 架构名，仅用于日志与诊断（不参与行为决策）。
pub const ARCH_NAME: &str = "riscv64";

/// 内核链接基址，与 linker/riscv64.ld 的 BASE_ADDRESS 一致。
///
/// 这两处必须同步：链接脚本是链接期的真相源，本常量只服务于日志与后续的地址校验。
/// 退出条件：M1 建立页表后改为从内核符号表读取，不再手工维护副本。
pub const LINK_BASE: usize = 0x8020_0000;

/// 让当前 hart 停止执行并等待中断。
///
/// M0 没有中断源，因此等效于永久停机。
/// 退出条件：M1 的 idle 线程与定时器中断接管后，这里改为调度器 idle 路径。
pub fn park() -> ! {
    loop {
        // SAFETY: wfi 只是提示 CPU 可进入低功耗等待，不读写内存、不需要栈。
        unsafe { core::arch::asm!("wfi", options(nomem, nostack)) };
    }
}
