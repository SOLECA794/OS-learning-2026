//! LoongArch64 trap 上下文及寄存器 ABI。

const PRMD_PPLV_MASK: usize = 0b11;
const USER_PLV: usize = 0b11;
const ESTAT_ECODE_SHIFT: usize = 16;
const ESTAT_ECODE_MASK: usize = 0x3f;

/// trap 时保存的完整整数现场。
///
/// `registers[n]` 对应 `$rN`；`$r0` 恒为零，`$r3` 保存 trap 前的原始栈指针。
/// 字段顺序是与汇编共享的 ABI，禁止在未同步汇编偏移时调整。
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TrapContext {
    registers: [usize; 32],
    prmd: usize,
    era: usize,
    estat: usize,
    badv: usize,
}

pub(super) const TRAP_CONTEXT_SIZE: usize = 36 * core::mem::size_of::<usize>();
const _: () = assert!(core::mem::size_of::<TrapContext>() == TRAP_CONTEXT_SIZE);

impl TrapContext {
    pub fn cause(&self) -> crate::kernel::trap::TrapCause {
        let code = (self.estat >> ESTAT_ECODE_SHIFT) & ESTAT_ECODE_MASK;
        match code {
            0 => crate::kernel::trap::TrapCause::Interrupt {
                code: self.estat & 0x1fff,
            },
            1 | 5 => crate::kernel::trap::TrapCause::LoadPageFault,
            2 | 4 => crate::kernel::trap::TrapCause::StorePageFault,
            3 | 6 => crate::kernel::trap::TrapCause::InstructionPageFault,
            11 => crate::kernel::trap::TrapCause::UserSyscall,
            12 => crate::kernel::trap::TrapCause::Breakpoint,
            13 => crate::kernel::trap::TrapCause::IllegalInstruction,
            _ => crate::kernel::trap::TrapCause::Unknown { code },
        }
    }

    pub const fn pc(&self) -> usize {
        self.era
    }
    pub const fn bad_address(&self) -> usize {
        self.badv
    }
    pub const fn is_from_user(&self) -> bool {
        self.prmd & PRMD_PPLV_MASK == USER_PLV
    }
    pub const fn syscall_number(&self) -> usize {
        self.registers[11]
    }

    pub const fn syscall_args(&self) -> [usize; 6] {
        [
            self.registers[4],
            self.registers[5],
            self.registers[6],
            self.registers[7],
            self.registers[8],
            self.registers[9],
        ]
    }

    pub fn set_syscall_return(&mut self, value: isize) {
        self.registers[4] = value as usize;
    }
    pub fn advance_pc(&mut self) {
        self.era = self.era.wrapping_add(4);
    }
}
