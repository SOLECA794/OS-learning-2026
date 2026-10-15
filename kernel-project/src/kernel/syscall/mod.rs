//! 系统调用统一分发入口。
//!
//! trap 层只负责从架构上下文中提取系统调用号和最多六个参数；Linux ABI 的参数校验、
//! errno 与具体语义由各分类模块负责。未知系统调用属于用户输入错误，必须返回 ENOSYS，
//! 不能 panic 使整个内核停止。
//!
//! 用户数据经 `copy_from_user` 逐页校验后复制到内核缓冲；非法指针返回 EFAULT。

pub const SYSCALL_REBOOT: usize = 142;
pub const SYSCALL_WRITE: usize = 64;
pub const SYSCALL_EXIT: usize = 93;
pub const SYSCALL_FORK: usize = 220;
pub const SYSCALL_WAIT4: usize = 260;
pub const SYSCALL_EXECVE: usize = 221;
pub const SYSCALL_DUP: usize = 23;
pub const SYSCALL_DUP2: usize = 24;
pub const SYSCALL_PIPE: usize = 59;
pub const SYSCALL_MKDIRAT: usize = 34;
pub const SYSCALL_OPENAT: usize = 56;
pub const SYSCALL_CLOSE: usize = 57;
pub const SYSCALL_READ: usize = 63;
pub const SYSCALL_LSEEK: usize = 62;
pub const SYSCALL_PREAD64: usize = 67;
pub const SYSCALL_PWRITE64: usize = 68;
pub const SYSCALL_FSTAT: usize = 80;
pub const SYSCALL_GETDENTS64: usize = 217;
pub const SYSCALL_BRK: usize = 214;
pub const SYSCALL_MUNMAP: usize = 215;
pub const SYSCALL_MMAP: usize = 222;

mod exec;
mod fd;
mod power;

pub use exec::sys_execve;
use power::sys_reboot;

/// Linux errno。
const ENOSYS: isize = 38;
const EBADF: isize = -9;
const EFAULT: isize = -14;
const ECHILD: isize = -10;
const ENOMEM: isize = -12;
const EINVAL: isize = -22;
/// wait4 options：WNOHANG（非阻塞）。
const WNOHANG: usize = 1;

/// 用户可写地址区间的下界（0 页不映射，NULL 解引用受控）。
const USER_TEXT_BASE: usize = 0x10000;
/// 用户地址空间上界由架构布局提供，并由逐页 PTE 检查进一步收紧。
const USER_STACK_TOP: usize = crate::arch::task::INITIAL_USER_STACK_TOP;

pub fn syscall(syscall_id: usize, args: [usize; 6]) -> isize {
    match syscall_id {
        // Linux reboot(2) ABI：a0=magic1, a1=magic2, a2=cmd, a3=arg
        SYSCALL_REBOOT => sys_reboot(args[0] as u32, args[1] as u32, args[2] as u32, args[3]),
        #[cfg(feature = "arch-riscv64")]
        SYSCALL_WRITE => fd::sys_write(args[0], args[1], args[2]),
        #[cfg(feature = "arch-loongarch64")]
        SYSCALL_WRITE => la_write(args[0], args[1], args[2]),
        SYSCALL_READ => fd::sys_read(args[0], args[1], args[2]),
        SYSCALL_LSEEK => fd::sys_lseek(args[0], args[1] as i64, args[2]),
        SYSCALL_PREAD64 => fd::sys_pread64(args[0], args[1], args[2], args[3] as i64),
        SYSCALL_PWRITE64 => fd::sys_pwrite64(args[0], args[1], args[2], args[3] as i64),
        #[cfg(feature = "arch-riscv64")]
        SYSCALL_FSTAT => fd::sys_fstat(args[0], args[1]),
        SYSCALL_CLOSE => fd::sys_close(args[0]),
        SYSCALL_OPENAT => fd::sys_openat(args[0], args[1], args[2], args[3]),
        #[cfg(feature = "arch-riscv64")]
        SYSCALL_MKDIRAT => fd::sys_mkdirat(args[0], args[1], args[2]),
        SYSCALL_DUP => fd::sys_dup(args[0]),
        #[cfg(feature = "arch-riscv64")]
        SYSCALL_DUP2 => fd::sys_dup2(args[0], args[1]),
        #[cfg(feature = "arch-riscv64")]
        SYSCALL_PIPE => fd::sys_pipe(args[0]),
        #[cfg(feature = "arch-riscv64")]
        SYSCALL_GETDENTS64 => fd::sys_getdents64(args[0], args[1], args[2]),
        // 两个架构都由共享 task owner 处理进程生命周期。
        SYSCALL_EXIT => crate::kernel::task::exit_current(args[0] as i32),
        // fork：返回子任务 id（>0）；子进程侧现场 a0 已置 0。
        SYSCALL_FORK => match crate::kernel::task::fork_current() {
            Ok(pid) => pid as isize,
            Err(errno) => errno,
        },
        // wait4(pid, wstatus, options, rusage)：仅 pid/wstatus/options，rusage 忽略。
        SYSCALL_WAIT4 => sys_wait4(args[0], args[1], args[2]),
        // brk(new_brk)：设置 program break；new_brk=0 返回当前 break。
        #[cfg(feature = "arch-riscv64")]
        SYSCALL_BRK => match crate::kernel::task::with_current_space(|space| space.brk(args[0])) {
            Some(Ok(addr)) => addr as isize,
            _ => -ENOMEM,
        },
        // mmap(addr, len, prot, flags, fd, offset)：当前只支持匿名 RW 映射。
        #[cfg(feature = "arch-riscv64")]
        SYSCALL_MMAP => {
            match crate::kernel::task::with_current_space(|space| space.mmap_anon(args[1])) {
                Some(Ok(addr)) => addr as isize,
                _ => -ENOMEM,
            }
        }
        // munmap(addr, len)：按起始地址解除映射。
        #[cfg(feature = "arch-riscv64")]
        SYSCALL_MUNMAP => {
            match crate::kernel::task::with_current_space(|space| space.munmap(args[0], args[1])) {
                Some(Ok(())) => 0,
                _ => -EINVAL,
            }
        }
        _ => -ENOSYS,
    }
}

/// LoongArch 控制台临时桥：保留原 stdout/stderr 行为；其他 FD 交给公共表。
/// 待控制台写入完成 FD 化验收后，移除此分支并统一调用 `fd::sys_write`。
#[cfg(feature = "arch-loongarch64")]
fn la_write(fd: usize, ptr: usize, len: usize) -> isize {
    if fd == 0 {
        return EBADF;
    }
    if fd != 1 && fd != 2 {
        return fd::sys_write(fd, ptr, len);
    }
    if len > crate::kernel::mm::address::PAGE_SIZE {
        return -22;
    }
    if len == 0 {
        return 0;
    }
    let bytes = match copy_from_user(ptr, len) {
        Ok(bytes) => bytes,
        Err(errno) => return errno,
    };
    crate::arch::console::kernel_arch_console_write(bytes.as_ptr(), bytes.len());
    len as isize
}

/// 校验用户地址区间和逐页 PTE，并返回由内核拥有的数据副本。
/// 不延长用户页引用生命周期；exec/fork/exit 后副本仍独立有效。
pub(crate) fn copy_from_user(src: usize, len: usize) -> Result<alloc::vec::Vec<u8>, isize> {
    if len == 0 {
        return Ok(alloc::vec::Vec::new());
    }
    let end = src.checked_add(len).ok_or(EFAULT)?;
    if src < USER_TEXT_BASE || end > USER_STACK_TOP {
        return Err(EFAULT);
    }
    crate::kernel::task::with_current_space(|space| space.copy_from_user(src, len))
        .ok_or(EFAULT)?
        .map_err(|_| EFAULT)
}

/// 从用户内存拷一个 null 终止字符串（有界）。
///
/// 逐页读取直到 null，避免字符串在当前页已结束时误触碰下一张未映射页。
/// 单次片段仍使用 `copy_from_user` 的地址及 PTE 检查。
pub(crate) fn copy_cstring_from_user(
    src: usize,
    max_len: usize,
) -> Result<alloc::vec::Vec<u8>, isize> {
    let mut bytes = alloc::vec::Vec::new();
    while bytes.len() < max_len {
        let addr = src.checked_add(bytes.len()).ok_or(EFAULT)?;
        let page_left = crate::kernel::mm::address::PAGE_SIZE
            - (addr & (crate::kernel::mm::address::PAGE_SIZE - 1));
        let n = page_left.min(max_len - bytes.len());
        let part = copy_from_user(addr, n)?;
        if let Some(end) = part.iter().position(|&b| b == 0) {
            bytes.extend_from_slice(&part[..end]);
            return Ok(bytes);
        }
        bytes.extend_from_slice(&part);
    }
    Err(EFAULT)
}
/// 向用户地址空间写字节：映射权限与 COW 由 MemorySet 统一判定。
pub(crate) fn copy_bytes_to_user(dst: usize, data: &[u8]) -> Result<(), isize> {
    if data.is_empty() {
        return Ok(());
    }
    let end = dst.checked_add(data.len()).ok_or(EFAULT)?;
    if dst < USER_TEXT_BASE || end > USER_STACK_TOP {
        return Err(EFAULT);
    }
    crate::kernel::task::with_current_space(|space| space.copy_to_user(dst, data))
        .ok_or(EFAULT)?
        .map_err(|_| EFAULT)
}

/// 写回 Linux `int` 宽度的 wait status；按字节复制，不形成未对齐用户引用。
fn copy_wait_status_to_user(dst: usize, value: i32) -> Result<(), isize> {
    copy_bytes_to_user(dst, &value.to_ne_bytes())
}

/// wait4(pid, wstatus, options)：阻塞循环——子已退出则先写 status，再提交回收；
/// 写回失败保留僵尸供重试。wstatus 按 Linux `int` 写入 4 字节。
/// 未退出则 Waiting 阻塞（子 exit 唤醒后重查），无子返回 ECHILD，WNOHANG 时不阻塞返回 0。
fn sys_wait4(pid: usize, status_ptr: usize, options: usize) -> isize {
    loop {
        match crate::kernel::task::wait4_step(pid as isize) {
            crate::kernel::task::WaitResult::Exited { pid, exit_code } => {
                if status_ptr != 0 {
                    let status = (exit_code & 0xff) << 8;
                    if copy_wait_status_to_user(status_ptr, status).is_err() {
                        return EFAULT;
                    }
                }
                crate::kernel::task::reap_child(pid);
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
