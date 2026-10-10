//! 系统调用统一分发入口。
//!
//! trap 层只负责从架构上下文中提取系统调用号和最多六个参数；Linux ABI 的参数校验、
//! errno 与具体语义由各分类模块负责。未知系统调用属于用户输入错误，必须返回 ENOSYS，
//! 不能 panic 使整个内核停止。

pub const SYSCALL_REBOOT: usize = 142;

mod power;

use power::sys_reboot;

/// Linux errno：系统调用号未实现。
const ENOSYS: isize = 38;

pub fn syscall(syscall_id: usize, args: [usize; 6]) -> isize {
    match syscall_id {
        // Linux reboot(2) ABI：
        //   a0 = magic1, a1 = magic2, a2 = cmd, a3 = arg
        SYSCALL_REBOOT => sys_reboot(args[0] as u32, args[1] as u32, args[2] as u32, args[3]),
        _ => -ENOSYS,
    }
}
