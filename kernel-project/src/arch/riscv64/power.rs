// ============================================================================
// RISC-V 平台系统关机 / 重启的底层实现
//
// 本模块通过 SBI（Supervisor Binary Interface）调用机器模式固件（如 OpenSBI），
// 请求其执行物理层面的关机或重启操作。
//
// 调用链：
//   用户态 reboot() / poweroff
//     -> 内核态系统调用处理
//       -> 本模块 system_reset()
//         -> ecall 指令陷入 M 模式
//           -> 固件执行物理电源操作
// ============================================================================

use core::arch::asm; // Rust 内联汇编宏，core 库不依赖操作系统，适合内核/裸机环境

// ----------------------------------------------------------------------------
// SBI 常量定义
// ----------------------------------------------------------------------------

/// SBI 系统重置扩展（SRST, System Reset Extension）的扩展 ID（EID）。
///
/// 0x5352_5354 是 ASCII 字符串 "SRST" 的十六进制编码：
///   'S' = 0x53, 'R' = 0x52, 'S' = 0x53, 'T' = 0x54
/// 这是 RISC-V SBI 规范为该扩展分配的固定标识。
const SBI_EXT_SRST: usize = 0x5352_5354;

/// SRST 扩展内的功能 ID（FID），对应 sbi_system_reset 功能。
///
/// 该功能用于请求系统级重启或关机。
/// 在 SRST 扩展中，这是唯一的功能，因此 FID 为 0。
const SBI_SRST_RESET: usize = 0;

/// 重置原因为“无特定原因”。
///
/// SBI 规范定义：
///   0 = 无特定原因（RESET_REASON_NONE）
///   1 = 系统故障（RESET_REASON_SYSTEM_FAILURE）
const RESET_REASON_NONE: usize = 0;

//sbi参数
const RESET_TYPE_SHUTDOWN: usize = 0;
const RESET_TYPE_COLD_REBOOT: usize = 1;

// ----------------------------------------------------------------------------
// SBI 调用返回值封装
// ----------------------------------------------------------------------------

/// 封装一次 SBI 调用的返回值。
///
/// SBI 调用约定规定，返回值通过 a0 和 a1 两个寄存器返回：
///   - a0：错误码（error），0 表示成功，负数表示失败
///   - a1：返回值（value），具体含义由各个 SBI 功能定义
#[derive(Debug, Clone, Copy)]
struct SbiRet {
    /// 错误码。0 表示成功，负数表示失败（如 -2 对应 ENOSYS，表示功能未实现）。
    error: isize,
    /// 返回值。对于 sbi_system_reset，该值通常为 0，但遵循 SBI 通用约定保留。
    value: usize,
}

// ----------------------------------------------------------------------------
// 核心函数：发起系统重置 SBI 调用
// ----------------------------------------------------------------------------

/// 通过 SBI 请求系统重置（关机或重启）。
///
/// # 参数
/// - `reset_type`：重置类型，决定执行关机还是重启。常见取值：
///     - `0`：关机（Shutdown）
///     - `1`：冷启动（Cold reboot，完整电源循环）
///     - `2`：热启动（Warm reboot，部分组件不断电）
/// - `reason`：重置原因。常用 `RESET_REASON_NONE`（0）表示无特定原因。
///
/// # 返回值
/// 返回 `SbiRet`，包含错误码和返回值。
///
/// **注意**：如果 `reset_type` 是关机或重启，SBI 固件执行物理操作后系统
/// 就关闭/重启了，**本函数通常不会返回**。只有在调用失败（如参数无效、
/// 功能未实现）时，才会返回并携带错误码。
///
/// # Safety
/// 调用者必须保证：
/// 1. 当前运行在 S 模式（Supervisor Mode）下。
/// 2. 底层固件已实现 SRST 扩展。
/// 3. 调用前已同步文件系统（sync），避免数据丢失。
unsafe fn system_reset(reset_type: usize, reason: usize) -> SbiRet {
    // 先声明输出变量，但不初始化。
    // 它们的值将由内联汇编的 lateout 部分写入。
    let error: isize;
    let value: usize;

    // SAFETY: 需要遵守 RISC-V SBI 的调用规则。
    // 我们确保：
    //   - 使用正确的寄存器（a7 = EID, a6 = FID, a0/a1 = 参数和返回值）
    //   - 通过 ecall 指令陷入 M 模式
    //   - 不读写栈内存（options(nostack)）
    unsafe {
        asm! {
            // ecall：RISC-V 环境调用指令。
            // 在 S 模式下执行会陷入 M 模式，由固件处理该请求。
            "ecall",

            // a0 寄存器：既是输入也是输出。
            // 输入：reset_type 的值放入 a0，告诉固件要执行哪种重置。
            // 输出：ecall 返回后，a0 中的值写入 error，即 SBI 调用的错误码。
            inlateout("a0") reset_type => error,

            // a1 寄存器：既是输入也是输出。
            // 输入：reason 的值放入 a1，表示重置原因。
            // 输出：a1 中的值写入 value，遵循 SBI 通用返回约定。
            inlateout("a1") reason => value,

            // a6 寄存器：SBI 调用的功能 ID（FID）。
            // 这里传入 SBI_SRST_RESET（0），表示调用 sbi_system_reset 功能。
            in("a6") SBI_SRST_RESET,

            // a7 寄存器：SBI 调用的扩展 ID（EID）。
            // 这里传入 SBI_EXT_SRST，表示使用系统重置扩展。
            in("a7") SBI_EXT_SRST,

            // 告诉编译器这段汇编不会读写栈内存，允许更激进的寄存器分配优化。
            options(nostack)
        };
    }

    // 将 SBI 调用的结果封装成 SbiRet 结构体返回。
    SbiRet { error, value }
}

pub fn restart() -> ! {
    // SAFETY: 参数使用 SBI SRST 规范定义的合法取值。
    let ret = unsafe { system_reset(RESET_TYPE_COLD_REBOOT, RESET_REASON_NONE) };

    // 成功时不会返回；能执行到这里说明 SBI 调用失败。
    crate::log_error!(
        "SBI cold reboot failed: error={}, value={}",
        ret.error,
        ret.value,
    );

    super::park()
}

pub fn power_off() -> ! {
    // SAFETY: 参数使用 SBI SRST 规范定义的合法取值。
    let ret = unsafe { system_reset(RESET_TYPE_SHUTDOWN, RESET_REASON_NONE) };

    crate::log_error!(
        "SBI shutdown failed: error={}, value={}",
        ret.error,
        ret.value,
    );

    super::park()
}
