//! 内核主流程（docs/01 §4 启动流程）。
//!
//! 当前启动链：
//!   arch 早期启动（汇编）→ kernel::main → 控制台/trap 初始化 → mm::init → 停机
//!
//! 后续步骤按 docs/01 §4 依次接入，都在 M1：
//!   kalloc::init → mm::init → driver::init/arch::init/fs::init → fs::mount_init_fs
//!   → task::create_initprocess → timer::init → arch::setup_all_cores → 调度器

/// 选主锁：放 `.data`（初值 0 随镜像加载，不在 BSS 清零范围内），
/// 保证无论哪个核先执行 `swap` 都只有一个成为 bootstrap。
///
/// 选主必须在清 BSS **之前**（P0① 修复）：汇编不再清 BSS，所有核进入
/// kernel_main 时 BSS 未清；主核 = swap 成功者，清 BSS 并初始化，从核直接
/// 停驻、不清 BSS，消除"迟到核重置主核已初始化全局"的条件性竞态。
#[link_section = ".data"]
static BOOT_LOCK: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

pub(crate) mod elf;
pub(crate) mod mm;
mod syscall;
pub(crate) mod trap;

/// 汇编入口 `kernel_main` 的 Rust 侧实现。
///
/// 用 `extern "C"` 是为了让参数落在两个架构的标准整型参数寄存器上
/// （RV: a0/a1；LA: $r4/$r5），这样两个后端可以共用同一份 `global_asm!` 调用约定。
///
/// # 参数语义
/// - `hartid`：bootstrap hart 的硬件编号。
/// - `dtb_pa`：DTB 物理地址。LoongArch 的 `-kernel` 入口不通过寄存器传递 DTB，
///   该架构下为 0，语义是「未提供」而不是「DTB 在物理地址 0」。
///   M1 handoff：统一为「先探测固件传参，再回落到固定位置」
#[no_mangle]
extern "C" fn kernel_main(hartid: usize, dtb_pa: usize) -> ! {
    // M0 临时桥：Rust 侧原子选主（汇编只做 per-hart 栈，清 BSS 移到主核专属，
    // 见 arch/*/boot.rs 与 clear_bss）。
    // 为什么在 Rust 侧：LLVM 对 global_asm 内联汇编的 Zaamo 指令检查与 Rust
    // 生成代码不一致（实测 amoswap 汇编被拒、AtomicBool 可编译）。
    // 行为边界：非 bootstrap 核直接停机，不清 BSS、不触碰控制台。
    // 移除条件：M1 arch::setup_all_cores 落地后，选主并入该路径。
    if BOOT_LOCK.swap(true, core::sync::atomic::Ordering::AcqRel) {
        crate::arch::park()
    }
    // 唯一主核：先清 BSS（此时尚无任何 BSS 全局被初始化），再进入主流程。
    clear_bss();
    main(hartid, dtb_pa)
}

/// 主核专属：清零 BSS。必须由唯一 bootstrap 在初始化任何 BSS 全局前执行。
///
/// 为什么在 Rust 侧：汇编清 BSS 与多核进入交错存在条件性竞态（规划书 P0①），
/// 先经 .data 的 BOOT_LOCK 选主、再由唯一主核清零，使清零唯一化且不与初始化交错。
fn clear_bss() {
    extern "C" {
        static __bss_start: u8;
        static __bss_end: u8;
    }
    let start = core::ptr::addr_of!(__bss_start) as usize;
    let end = core::ptr::addr_of!(__bss_end) as usize;
    let mut p = start;
    // SAFETY: __bss_start/__bss_end 由链接脚本保证落在可写 RAM 且区间不重叠任何
    // 正在使用的数据；清零目标为 BSS，volatile 防编译器合并/消除。
    // 8 字节块清，尾部不足 8 字节按字节清（__bss_end 由链接脚本定义）。
    unsafe {
        while p + 8 <= end {
            (p as *mut usize).write_volatile(0);
            p += 8;
        }
        while p < end {
            (p as *mut u8).write_volatile(0);
            p += 1;
        }
    }
    core::sync::atomic::fence(core::sync::atomic::Ordering::Release);
}

/// 内核主函数。M0 结束在停机，不做任何"假装继续跑"的事。
pub fn main(hartid: usize, dtb_pa: usize) -> ! {
    crate::arch::console::init();
    crate::arch::trap::init();

    crate::log_info!("=============================================");
    crate::log_info!(" kernel-project — 从 0 自研 Linux 兼容宏内核");
    crate::log_info!(" 里程碑：M1（trap 骨架、MemorySet、内核分页）");
    crate::log_info!("=============================================");
    crate::log_info!(
        "arch={} link_base={:#x} hartid={} dtb_pa={:#x} (0 = 未提供)",
        crate::arch::ARCH_NAME,
        crate::arch::LINK_BASE,
        hartid,
        dtb_pa,
    );
    mm::init();
    crate::log_info!("M1 paging/trap foundation OK");
    crate::log_info!("M1.5: entering user mode");
    init_user()
}

/// 加载内嵌 uapp，构造用户地址空间与内核栈，进入用户态（M1.5 单任务闭环）。
///
/// 流程：ELF 解析 → 用户 MemorySet（内核映射+用户段+用户栈）→ 拷段内容 →
/// 激活用户页表 → 内核栈顶放置 TrapContext → sscratch=栈顶 → restore 进 U 模式。
fn init_user() -> ! {
    // 默认内嵌 uapp.elf；`make test-fault` 时经 feature 切换为 uapp_fault.elf
    // （用户态访问内核保护区的独立测试程序，见 W2 附加用例 change record）。
    #[cfg(feature = "uapp-fault-test")]
    const UAPP_ELF: &[u8] = include_bytes!("../../build/user/uapp_fault.elf");
    #[cfg(not(feature = "uapp-fault-test"))]
    const UAPP_ELF: &[u8] = include_bytes!("../../build/user/uapp.elf");
    let uapp: &[u8] = UAPP_ELF;
    let elf = crate::kernel::elf::parse(uapp).expect("uapp ELF parse failed");

    let layout = mm::kernel_space::KernelImageLayout::current();
    let memory_bytes = mm::configured_memory_bytes();
    let (_, memory_end) = crate::arch::mm::physical_memory_range(memory_bytes)
        .expect("configured memory range unavailable");

    // 用户栈顶固定低于内核身份映射区（RV: 内核 0x8000_0000 起）。
    const USER_STACK_TOP: usize = 0x7f00_0000;
    let user_stack = mm::address::AddressRange::new(
        mm::address::VirtAddr::new(USER_STACK_TOP - 0x1000),
        mm::address::VirtAddr::new(USER_STACK_TOP),
    )
    .expect("user stack range");

    let mut space = mm::memory_set::MemorySet::new_user(
        layout,
        mm::address::VirtAddr::new(memory_end),
        &elf.segments,
        user_stack,
    )
    .expect("user MemorySet construction failed");
    space
        .load_elf(&elf.segments, uapp)
        .expect("uapp segment load failed");

    let kstack = mm::kernel_stack::KernelStack::new().expect("user task kernel stack");
    let kstack_top = kstack.top();

    // SAFETY: 用户页表含内核身份映射 + 用户段，切表后当前执行地址与栈仍可访问。
    unsafe { crate::arch::mm::activate(space.root_ppn().value()) };

    let context = crate::arch::trap::TrapContext::new_user(elf.entry, USER_STACK_TOP);
    let context_ptr = (kstack_top - core::mem::size_of::<crate::arch::trap::TrapContext>())
        as *mut crate::arch::trap::TrapContext;
    // SAFETY: context_ptr 位于新分配内核栈顶下方，无重叠使用。
    unsafe { context_ptr.write(context) };
    crate::arch::trap::set_user_scratch(kstack_top);
    crate::log_info!(
        "M1.5: running user program entry={:#x} user_sp={:#x} kernel_stack_top={:#x}",
        elf.entry,
        USER_STACK_TOP,
        kstack_top,
    );
    crate::arch::trap::restore_from(context_ptr)
}
