//! 内核主流程（docs/01 §4 启动流程）。
//!
//! M0 只交付「boot 到 init」这一段：
//!   arch 早期启动（汇编）→ kernel::main → 控制台初始化 → banner → 停机
//!
//! 后续步骤按 docs/01 §4 依次接入，都在 M1：
//!   kalloc::init → mm::init → driver::init/arch::init/fs::init → fs::mount_init_fs
//!   → task::create_initprocess → timer::init → arch::setup_all_cores → 调度器

/// 选主锁：放 `.data`（初值 0 随镜像加载，不在 BSS 清零范围内），
/// 保证无论哪个核先执行 `swap` 都只有一个成为 bootstrap。
#[link_section = ".data"]
static BOOT_LOCK: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// 汇编入口 `kernel_main` 的 Rust 侧实现。
///
/// 用 `extern "C"` 是为了让参数落在两个架构的标准整型参数寄存器上
/// （RV: a0/a1；LA: $r4/$r5），这样两个后端可以共用同一份 `global_asm!` 调用约定。
///
/// # 参数语义
/// - `hartid`：bootstrap hart 的硬件编号。
/// - `dtb_pa`：DTB 物理地址。LoongArch 的 `-kernel` 入口不通过寄存器传递 DTB，
///   该架构下为 0，语义是「未提供」而不是「DTB 在物理地址 0」。
///   M1 handoff：统一为「先探测固件传参，再回落到固定位置」。
#[no_mangle]
extern "C" fn kernel_main(hartid: usize, dtb_pa: usize) -> ! {
    // M0 临时桥：Rust 侧原子选主（汇编只做 per-hart 栈与清 BSS，见 arch/*/boot.rs）。
    // 为什么在 Rust 侧：LLVM 对 global_asm 内联汇编的 Zaamo 指令检查与 Rust
    // 生成代码不一致（实测 amoswap 汇编被拒、AtomicBool 可编译）。
    // 行为边界：非 bootstrap 核直接停机，不触碰控制台与 BSS。
    // 移除条件：M1 arch::setup_all_cores 落地后，选主并入该路径。
    if BOOT_LOCK.swap(true, core::sync::atomic::Ordering::AcqRel) {
        crate::arch::park()
    }
    main(hartid, dtb_pa)
}

/// 内核主函数。M0 结束在停机，不做任何"假装继续跑"的事。
pub fn main(hartid: usize, dtb_pa: usize) -> ! {
    crate::arch::console::init();

    crate::log_info!("=============================================");
    crate::log_info!(" kernel-project — 从 0 自研 Linux 兼容宏内核");
    crate::log_info!(" 里程碑：M0（地基：规范、工具链、双架构 boot）");
    crate::log_info!("=============================================");
    crate::log_info!(
        "arch={} link_base={:#x} hartid={} dtb_pa={:#x} (0 = 未提供)",
        crate::arch::ARCH_NAME,
        crate::arch::LINK_BASE,
        hartid,
        dtb_pa,
    );
    crate::log_info!("M0 boot OK");
    crate::log_info!("下一步（M1）：kalloc::init → mm::init → trap/syscall 骨架");
    crate::log_info!("进入停机：M0 没有中断、没有调度器，没有可运行的东西。");

    crate::arch::park()
}
