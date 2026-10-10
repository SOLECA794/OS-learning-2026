//! RISC-V64 trap 上下文及寄存器 ABI。

const SSTATUS_SPP: usize = 1 << 8;
const SCAUSE_INTERRUPT: usize = 1 << 63;
const SCAUSE_CODE_MASK: usize = !SCAUSE_INTERRUPT;

/// trap 时保存的完整整数现场。
///
/// `registers[n]` 对应 `xN`；`x0` 保留为零，`x2` 保存 trap 前的原始 `sp`。
/// 字段顺序是与汇编共享的 ABI，禁止在未同步汇编偏移时调整。
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TrapContext {
    registers: [usize; 32],
    sstatus: usize,
    sepc: usize,
    scause: usize,
    stval: usize,
}

pub(super) const TRAP_CONTEXT_SIZE: usize = 36 * core::mem::size_of::<usize>();
const _: () = assert!(core::mem::size_of::<TrapContext>() == TRAP_CONTEXT_SIZE);

impl TrapContext {
    pub fn cause(&self) -> crate::kernel::trap::TrapCause {
        let code = self.scause & SCAUSE_CODE_MASK;
        if self.scause & SCAUSE_INTERRUPT != 0 {
            return crate::kernel::trap::TrapCause::Interrupt { code };
        }

        match code {
            2 => crate::kernel::trap::TrapCause::IllegalInstruction,
            3 => crate::kernel::trap::TrapCause::Breakpoint,
            8 => crate::kernel::trap::TrapCause::UserSyscall,
            12 => crate::kernel::trap::TrapCause::InstructionPageFault,
            13 => crate::kernel::trap::TrapCause::LoadPageFault,
            15 => crate::kernel::trap::TrapCause::StorePageFault,
            _ => crate::kernel::trap::TrapCause::Unknown { code },
        }
    }

    pub const fn pc(&self) -> usize {
        self.sepc
    }
    pub const fn bad_address(&self) -> usize {
        self.stval
    }
    pub const fn is_from_user(&self) -> bool {
        self.sstatus & SSTATUS_SPP == 0
    }
    pub const fn syscall_number(&self) -> usize {
        self.registers[17]
    }

    pub const fn syscall_args(&self) -> [usize; 6] {
        [
            self.registers[10],
            self.registers[11],
            self.registers[12],
            self.registers[13],
            self.registers[14],
            self.registers[15],
        ]
    }

    pub fn set_syscall_return(&mut self, value: isize) {
        self.registers[10] = value as usize;
    }
    pub fn advance_pc(&mut self) {
        self.sepc = self.sepc.wrapping_add(4);
    }
}
