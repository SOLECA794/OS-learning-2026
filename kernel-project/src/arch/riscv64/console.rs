//! RISC-V64 控制台：走 OpenSBI 的 legacy console_putchar。
//!
//! 为什么用 legacy 而不是 DBCN（0x4442434E）：
//! legacy EID 0x01 被所有 OpenSBI 版本支持，boot 早期不需要探测扩展是否存在；
//! DBCN 需要 `sbi_probe_extension`，那是 M1 建立 SBI 封装层之后的事。
//! 退出条件：M1 引入统一 SBI 封装后改为"优先 DBCN，fallback legacy"。

/// legacy SBI 扩展 ID：console_putchar。
const SBI_LEGACY_CONSOLE_PUTCHAR: usize = 0x01;

/// SBI legacy 调用约定：a7 = 扩展 ID，a6 = 功能 ID（legacy 固定 0），a0 = 参数。
unsafe fn sbi_putchar(ch: u8) {
    unsafe {
        core::arch::asm!(
            "ecall",
            inlateout("a0") ch as usize => _,
            in("a6") 0usize,
            in("a7") SBI_LEGACY_CONSOLE_PUTCHAR,
            // 不声明 nomem：ecall 有外部可见副作用。
            options(nostack),
        );
    }
}

/// 初始化控制台。
///
/// RISC-V 侧为空操作：OpenSBI 的控制台在固件阶段就可用。
/// 保留此函数是为了让 `kernel::main` 的启动序列对两个架构一致（LA 侧确实需要配置 UART）。
pub fn init() {}

/// klib::log 的字节输出钩子。
///
/// # Safety
/// `ptr` 必须指向至少 `len` 字节的可读内存。调用方（klib::log）保证来自 Rust 切片。
#[no_mangle]
pub extern "C" fn kernel_arch_console_write(ptr: *const u8, len: usize) {
    // SAFETY: 契约见函数文档，唯一调用方是 klib::log::write_bytes。
    let bytes = unsafe { core::slice::from_raw_parts(ptr, len) };
    for &b in bytes {
        // SAFETY: ecall 无前置条件，寄存器由 asm 显式约束。
        unsafe { sbi_putchar(b) };
    }
}
