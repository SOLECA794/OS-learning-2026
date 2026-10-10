//! LoongArch64 后端。所有 LoongArch 特有的 CSR/汇编只允许出现在本目录内。

pub mod boot;
pub mod console;

/// 架构名，仅用于日志与诊断（不参与行为决策）。
pub const ARCH_NAME: &str = "loongarch64";

/// 内核链接基址，与 linker/loongarch64.ld 的 BASE_ADDRESS 一致。
///
/// 必须落在 QEMU virt 的低 256MB RAM 段内；放到 0x80000000 会让 BSS 跨越
/// lowmem/highmem 之间的空洞（见 linker/loongarch64.ld 顶部注释）。
pub const LINK_BASE: usize = 0x0020_0000;

/// 让当前 hart 停止执行并等待中断。
///
/// M0 没有中断控制器配置，因此等效于永久停机。
/// 退出条件：M1 接入 LS7A/EIOINTC 与 idle 线程后，这里改为调度器 idle 路径。
pub fn park() -> ! {
    loop {
        // SAFETY: idle 只是提示 CPU 等待中断，不读写内存、不需要栈。
        unsafe { core::arch::asm!("idle 0", options(nomem, nostack)) };
    }
}
