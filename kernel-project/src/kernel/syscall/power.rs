//! Linux `reboot(2)` 系统调用的架构无关语义。
//!
//! 本模块负责 Linux UAPI 的权限、magic number、命令与 errno，并通过 `crate::arch`
//! 的统一接口执行最终动作；不能在 syscall 层直接依赖 SBI 或平台 MMIO。

/// Linux errno：当前调用者没有执行该操作的权限。
const EPERM: isize = 1;
/// Linux errno：参数或命令无效。
const EINVAL: isize = 22;

// magic number 用来避免用户程序因寄存器或系统调用号错误而意外重启整台机器。
const LINUX_REBOOT_MAGIC1: u32 = 0xfee1_dead;
const LINUX_REBOOT_MAGIC2: u32 = 672_274_793;
const LINUX_REBOOT_MAGIC2A: u32 = 85_072_278;
const LINUX_REBOOT_MAGIC2B: u32 = 369_367_448;
const LINUX_REBOOT_MAGIC2C: u32 = 537_993_216;

// Linux UAPI 定义的 reboot 命令。当前最小实现只执行 RESTART、HALT 和 POWER_OFF；
// CAD 与 RESTART2 的常量仍保留在 ABI 边界，以便明确返回 EINVAL 而不是误执行其它动作。
const LINUX_REBOOT_CMD_CAD_OFF: u32 = 0x0000_0000;
const LINUX_REBOOT_CMD_RESTART: u32 = 0x0123_4567;
const LINUX_REBOOT_CMD_HALT: u32 = 0xcdef_0123;
const LINUX_REBOOT_CMD_CAD_ON: u32 = 0x89ab_cdef;
const LINUX_REBOOT_CMD_POWER_OFF: u32 = 0x4321_fedc;
const LINUX_REBOOT_CMD_RESTART2: u32 = 0xa1b2_c3d4;

/// 处理 Linux `reboot(2)`。
///
/// 参数依次对应用户态传入的 `magic1`、`magic2`、`cmd` 和 `arg`。成功执行重启、关机或
/// 停机后函数不会返回；可恢复的 ABI 错误以负 errno 返回给用户态。
pub fn sys_reboot(magic1: u32, magic2: u32, cmd: u32, _arg: usize) -> isize {
    // Linux 要求调用者具有 CAP_SYS_BOOT。当前 M1 尚未落地 credentials/capability，
    // may_reboot() 是有明确退出条件的临时策略，不能在权限系统落地后继续保留。
    if !may_reboot() {
        return -EPERM;
    }

    if magic1 != LINUX_REBOOT_MAGIC1 || !valid_magic2(magic2) {
        return -EINVAL;
    }

    match cmd {
        // arch 层负责把统一动作映射为 RISC-V SBI 或其它架构的平台机制。
        LINUX_REBOOT_CMD_RESTART => crate::arch::restart(),
        LINUX_REBOOT_CMD_POWER_OFF => crate::arch::power_off(),

        // HALT 只停止内核运行，不要求切断电源。
        LINUX_REBOOT_CMD_HALT => {
            crate::log_info!("system halted");
            crate::arch::park()
        }

        // CAD 需要键盘/信号状态机；RESTART2 需要安全地读取 arg 指向的用户字符串。
        // 这些依赖尚未落地，当前必须显式拒绝，不能静默降级为普通重启。
        LINUX_REBOOT_CMD_CAD_ON | LINUX_REBOOT_CMD_CAD_OFF | LINUX_REBOOT_CMD_RESTART2 => -EINVAL,

        _ => -EINVAL,
    }
}

fn valid_magic2(magic: u32) -> bool {
    matches!(
        magic,
        LINUX_REBOOT_MAGIC2 | LINUX_REBOOT_MAGIC2A | LINUX_REBOOT_MAGIC2B | LINUX_REBOOT_MAGIC2C
    )
}

/// 临时权限策略。
///
/// 保留原因：当前没有 Task credentials 与 capability 模型，无法查询 CAP_SYS_BOOT。
/// 行为边界：M1 只有受控的内核启动路径会调用 syscall 分发器。
/// 移除条件：task credentials 落地后改为检查调用者的 CAP_SYS_BOOT；届时无权限返回 EPERM。
fn may_reboot() -> bool {
    true
}
