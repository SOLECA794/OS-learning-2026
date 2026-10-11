//! 系统调用统一分发入口。
//!
//! trap 层只负责从架构上下文中提取系统调用号和最多六个参数；Linux ABI 的参数校验、
//! errno 与具体语义由各分类模块负责。未知系统调用属于用户输入错误，必须返回 ENOSYS，
//! 不能 panic 使整个内核停止。
//!
//! M1.5 新增 write/exit（用户态首闭环）；用户指针经 `copy_from_user` 段边界校验，
//! 非法指针返回 EFAULT（受控，不 panic）。

pub const SYSCALL_REBOOT: usize = 142;
pub const SYSCALL_WRITE: usize = 64;
pub const SYSCALL_EXIT: usize = 93;

mod power;

use power::sys_reboot;

/// Linux errno。
const ENOSYS: isize = 38;
const EBADF: isize = -9;
const EFAULT: isize = -14;

/// 用户可写地址区间的下界（0 页不映射，NULL 解引用受控）。
const USER_TEXT_BASE: usize = 0x10000;
/// 用户地址空间上界（RV：低于内核身份映射区 0x8000_0000）。
const USER_STACK_TOP: usize = 0x7f00_0000;

pub fn syscall(syscall_id: usize, args: [usize; 6]) -> isize {
    match syscall_id {
        // Linux reboot(2) ABI：a0=magic1, a1=magic2, a2=cmd, a3=arg
        SYSCALL_REBOOT => sys_reboot(args[0] as u32, args[1] as u32, args[2] as u32, args[3]),
        SYSCALL_WRITE => sys_write(args[0], args[1], args[2]),
        SYSCALL_EXIT => sys_exit(args[0]),
        _ => -ENOSYS,
    }
}

/// 校验用户指针区间并返回只读视图。
///
/// 当前激活页表含用户段映射，且 trap 处理期间 sstatus.SUM=1（见 entry 汇编），
/// 因此 S-mode 可直接解引用用户页。M1.5 以段边界校验为准（单任务、无 fork），
/// M1.6 换为逐页 PTE 权限查询（boundary M1.5 §4 已登记）。
fn copy_from_user(src: usize, len: usize) -> Result<&'static [u8], isize> {
    if len == 0 {
        return Ok(&[]);
    }
    let end = src.checked_add(len).ok_or(EFAULT)?;
    if src < USER_TEXT_BASE || end > USER_STACK_TOP {
        return Err(EFAULT);
    }
    // SAFETY: 边界已校验；当前单任务、无回收，'static 延长安全（登记见 boundary）。
    Ok(unsafe { core::slice::from_raw_parts(src as *const u8, len) })
}

/// write(fd, buf, len)：仅支持 fd=1（控制台输出），直接逐字节写不缓冲。
fn sys_write(fd: usize, buf: usize, len: usize) -> isize {
    if fd != 1 {
        return -EBADF;
    }
    let data = match copy_from_user(buf, len) {
        Ok(data) => data,
        Err(error) => return error,
    };
    if !data.is_empty() {
        crate::arch::console::kernel_arch_console_write(data.as_ptr(), data.len());
    }
    len as isize
}

/// exit(code)：记录退出并停机（M1.5 单任务无调度；M3 换为回收 + 调度）。
fn sys_exit(code: usize) -> isize {
    crate::log_info!("user program exited with code {}", code);
    crate::arch::park()
}
