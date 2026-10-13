//! 架构无关的 trap 处理入口。
//!
//! 架构层负责保存寄存器并把硬件原因归一化为 `TrapCause`；本模块拥有处理策略。
//! 当前只接管来自用户态的系统调用。尚无 Task、用户态内核栈和中断子系统，
//! 因此其余 trap 都打印现场后安全停机。

/// 两个架构向内核提交的统一 trap 原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TrapCause {
    UserSyscall,
    Breakpoint,
    IllegalInstruction,
    InstructionPageFault,
    LoadPageFault,
    StorePageFault,
    Interrupt { code: usize },
    Unknown { code: usize },
}

/// 架构汇编入口的 Rust handoff。
///
/// `#[no_mangle]` 与 C ABI 保证两个架构的汇编都能用固定符号和指针参数调用这里。
#[no_mangle]
extern "C" fn kernel_trap_handler(context: &mut crate::arch::TrapContext) {
    handle_trap(context)
}

fn handle_trap(context: &mut crate::arch::TrapContext) {
    let cause = context.cause();
    match cause {
        TrapCause::UserSyscall if context.is_from_user() => handle_user_syscall(context),
        TrapCause::UserSyscall => fatal_trap(context, cause, "syscall did not come from user mode"),
        // V2 G6：仅接管写时复制（COW）页的 store fault；其余缺页仍受控停机。
        // LA 侧无 task 模块（COW 用户态尚未对齐），store fault 一律受控停机。
        TrapCause::StorePageFault => {
            #[cfg(feature = "arch-riscv64")]
            if !handle_store_fault(context) {
                fatal_trap(context, cause, "store fault outside COW pages")
            }
            #[cfg(not(feature = "arch-riscv64"))]
            fatal_trap(context, cause, "page-fault handling is not implemented")
        }
        TrapCause::InstructionPageFault | TrapCause::LoadPageFault => {
            fatal_trap(context, cause, "page-fault handling is not implemented")
        }
        #[cfg(feature = "arch-riscv64")]
        TrapCause::Interrupt { code } if code == 5 => {
            // S 态时钟中断（STI）：重设下一次触发后进入调度（先用户态抢占）。
            // 内核态不抢占：handler 期间 sstatus.SIE=0，不会在此路径嵌套。
            crate::arch::timer::set_next_trigger();
            crate::kernel::task::schedule();
        }
        TrapCause::Interrupt { .. } => fatal_trap(context, cause, "unhandled interrupt"),
        TrapCause::Breakpoint => {
            fatal_trap(context, cause, "breakpoint handling is not implemented")
        }
        TrapCause::IllegalInstruction => fatal_trap(context, cause, "illegal instruction"),
        TrapCause::Unknown { .. } => fatal_trap(context, cause, "unknown trap cause"),
    }
}

/// 从统一寄存器接口取出 Linux 风格系统调用 ABI，并把返回值写回用户寄存器。
fn handle_user_syscall(context: &mut crate::arch::TrapContext) {
    let syscall_id = context.syscall_number();
    let args = context.syscall_args();

    // 两个架构的异常 PC 都指向 syscall/ecall 指令本身。必须先越过 4 字节指令，
    // 否则可返回的系统调用会在 ertn/sret 后再次执行并形成无限 trap。
    // 先推进 PC 也保证 restart 这类 `-> !` 路径不依赖后续现场修改。
    context.advance_pc();
    // exec 是唯一"成功不返回旧现场"的 syscall：成功后现场已被重写为新程序
    // （entry/argc/argv/新栈），不得再写返回值（V2 §7 返回动作合同）。
    #[cfg(feature = "arch-riscv64")]
    if syscall_id == super::syscall::SYSCALL_EXECVE {
        match super::syscall::sys_execve(context, args) {
            Ok(()) => return,
            Err(errno) => {
                context.set_syscall_return(errno);
                return;
            }
        }
    }
    let result = super::syscall::syscall(syscall_id, args);
    context.set_syscall_return(result);
}

/// 处理用户/内核态对 COW 页的写访问：分配新页、拷贝、解除 COW、恢复写权限。
/// 返回 true=已处理（sepc 不变，重新执行故障指令）；false=非 COW 页（调用方 fatal）。
#[cfg(feature = "arch-riscv64")]
fn handle_store_fault(context: &crate::arch::TrapContext) -> bool {
    use crate::kernel::mm::address::VirtAddr;
    let vpn = VirtAddr::new(context.bad_address()).floor();
    match crate::kernel::task::with_current_space(|space| space.handle_cow_store(vpn)) {
        Some(Ok(true)) => true,
        _ => false,
    }
}

fn fatal_trap(context: &crate::arch::TrapContext, cause: TrapCause, reason: &'static str) -> ! {
    crate::log_error!(
        "fatal trap: reason={} cause={:?} pc={:#x} bad_addr={:#x} sp={:#x} from_user={}",
        reason,
        cause,
        context.pc(),
        context.bad_address(),
        context.sp(),
        context.is_from_user(),
    );

    // 未实现的 trap 不能返回并重复执行故障指令，也不能伪造处理成功。
    crate::arch::park()
}
