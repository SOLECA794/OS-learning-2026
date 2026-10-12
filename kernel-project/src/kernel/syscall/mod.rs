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
pub const SYSCALL_FORK: usize = 220;
pub const SYSCALL_WAIT4: usize = 260;
pub const SYSCALL_EXECVE: usize = 221;
pub const SYSCALL_DUP: usize = 23;
pub const SYSCALL_OPENAT: usize = 56;
pub const SYSCALL_CLOSE: usize = 57;
pub const SYSCALL_READ: usize = 63;

#[cfg(feature = "arch-riscv64")]
mod exec;
#[cfg(feature = "arch-riscv64")]
mod fd;
mod power;

#[cfg(feature = "arch-riscv64")]
pub use exec::sys_execve;
use power::sys_reboot;

/// Linux errno。
const ENOSYS: isize = 38;
const EBADF: isize = -9;
const EFAULT: isize = -14;
const ECHILD: isize = -10;
/// wait4 options：WNOHANG（非阻塞）。
const WNOHANG: usize = 1;

/// 用户可写地址区间的下界（0 页不映射，NULL 解引用受控）。
const USER_TEXT_BASE: usize = 0x10000;
/// 用户地址空间上界（RV：低于内核身份映射区 0x8000_0000）。
const USER_STACK_TOP: usize = 0x7f00_0000;

pub fn syscall(syscall_id: usize, args: [usize; 6]) -> isize {
    match syscall_id {
        // Linux reboot(2) ABI：a0=magic1, a1=magic2, a2=cmd, a3=arg
        SYSCALL_REBOOT => sys_reboot(args[0] as u32, args[1] as u32, args[2] as u32, args[3]),
        #[cfg(feature = "arch-riscv64")]
        SYSCALL_WRITE => fd::sys_write(args[0], args[1], args[2]),
        #[cfg(feature = "arch-riscv64")]
        SYSCALL_READ => fd::sys_read(args[0], args[1], args[2]),
        #[cfg(feature = "arch-riscv64")]
        SYSCALL_CLOSE => fd::sys_close(args[0]),
        #[cfg(feature = "arch-riscv64")]
        SYSCALL_OPENAT => fd::sys_openat(args[0], args[1], args[2]),
        #[cfg(feature = "arch-riscv64")]
        SYSCALL_DUP => fd::sys_dup(args[0]),
        // exit/fork/wait4 依赖 task 模块（RV 用户态调度）；LA 用户态未实现，回落 ENOSYS。
        #[cfg(feature = "arch-riscv64")]
        SYSCALL_EXIT => crate::kernel::task::exit_current(args[0] as i32),
        // fork：返回子任务 id（>0）；子进程侧现场 a0 已置 0。
        #[cfg(feature = "arch-riscv64")]
        SYSCALL_FORK => match crate::kernel::task::fork_current() {
            Ok(pid) => pid as isize,
            Err(errno) => errno,
        },
        // wait4(pid, wstatus, options, rusage)：仅 pid/wstatus/options，rusage 忽略。
        #[cfg(feature = "arch-riscv64")]
        SYSCALL_WAIT4 => sys_wait4(args[0], args[1], args[2]),
        _ => -ENOSYS,
    }
}

/// 校验用户指针区间并返回只读视图。
///
/// 当前激活页表含用户段映射，且 trap 处理期间 sstatus.SUM=1（见 entry 汇编），
/// 因此 S-mode 可直接解引用用户页。M1.5 以段边界校验为准（单任务、无 fork），
/// M1.6 换为逐页 PTE 权限查询（boundary M1.5 §4 已登记）。
pub(crate) fn copy_from_user(src: usize, len: usize) -> Result<&'static [u8], isize> {
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

/// 从用户内存拷一个 null 终止字符串（有界）。
///
/// 不能一次拷满 max_len：字符串可能位于用户栈顶附近，src+max_len 会越出
/// USER_STACK_TOP（G5 F1 实机发现）。按「可用区间 = min(max_len, STACK_TOP-src)」
/// 一次段校验；区间内无 null 视为超长 → EFAULT。
pub(crate) fn copy_cstring_from_user(
    src: usize,
    max_len: usize,
) -> Result<alloc::vec::Vec<u8>, isize> {
    let avail = USER_STACK_TOP.saturating_sub(src).min(max_len);
    let buf = copy_from_user(src, avail)?;
    let end = buf.iter().position(|&b| b == 0).ok_or(EFAULT)?;
    Ok(buf[..end].to_vec())
}
/// 向用户内存写一个 usize（wait status 等）。段边界校验同 copy_from_user；
/// SUM=1 处理期内核可直接写用户页。
fn copy_to_user(dst: usize, value: usize) -> Result<(), isize> {
    let end = dst
        .checked_add(core::mem::size_of::<usize>())
        .ok_or(EFAULT)?;
    if dst < USER_TEXT_BASE || end > USER_STACK_TOP {
        return Err(EFAULT);
    }
    // SAFETY: 边界已校验且当前激活页表含用户段映射（SUM=1）。
    unsafe { (dst as *mut usize).write(value) };
    Ok(())
}

/// wait4(pid, wstatus, options)：阻塞循环——子已退出则回收并写 status（WEXITSTATUS 编码），
/// 未退出则 Waiting 阻塞（子 exit 唤醒后重查），无子返回 ECHILD，WNOHANG 时不阻塞返回 0。
#[cfg(feature = "arch-riscv64")]
fn sys_wait4(pid: usize, status_ptr: usize, options: usize) -> isize {
    loop {
        match crate::kernel::task::wait4_step(pid as isize) {
            crate::kernel::task::WaitResult::Reaped { pid, exit_code } => {
                if status_ptr != 0 {
                    let status = ((exit_code & 0xff) as usize) << 8;
                    if copy_to_user(status_ptr, status).is_err() {
                        return EFAULT;
                    }
                }
                return pid as isize;
            }
            crate::kernel::task::WaitResult::WouldBlock => {
                if options & WNOHANG != 0 {
                    return 0;
                }
                // 阻塞：Waiting + 调度切走；子 exit 唤醒后重查。
                crate::kernel::task::block_current_and_wait();
            }
            crate::kernel::task::WaitResult::NoChild => return ECHILD,
        }
    }
}
