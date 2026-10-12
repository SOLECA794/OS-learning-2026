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
        TrapCause::InstructionPageFault | TrapCause::LoadPageFault | TrapCause::StorePageFault => {
            fatal_trap(context, cause, "page-fault handling is not implemented")
        }
        #[cfg(feature = "arch-riscv64")]
        TrapCause::Interrupt { code } if code == 5 => {
            // S 态时钟中断（STI）：重设下一次触发后进入调度（先用户态抢占）。
            // 内核态不抢占：handler 期间 sstatus.SIE=0，不会在此路径嵌套。
            crate::arch::timer::set_next_trigger();
            crate::kernel::task::schedule();
        }
        TrapCause::Interrupt { .. } => {
            fatal_trap(context, cause, "unhandled interrupt")
        }
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
    let result = super::syscall::syscall(syscall_id, args);
    context.set_syscall_return(result);
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
