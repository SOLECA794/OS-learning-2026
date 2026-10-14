//! execve(2)（RV，G4）：按路径从命名程序源加载新程序并一次性替换现场。
//!
//! V2 §7 合同：exec 成功必须"使用新现场"，不能往旧 TrapContext 写返回值——
//! 成功后现场被重写为（entry, argc, argv, 新栈），trap handler 返回即进新程序；
//! 失败保留旧进程（旧地址空间与旧现场不变），返回 errno。

use alloc::vec::Vec;

use crate::arch;
use crate::kernel::elf;
use crate::kernel::mm::address::{AddressRange, VirtAddr};
use crate::kernel::mm::memory_set::MemorySet;
use crate::kernel::syscall::copy_from_user;

const USER_STACK_TOP: usize = 0x7f00_0000;
/// 用户 argv 数组最多条目数（含 NULL 结尾）。
const MAX_ARGV: usize = 16;
/// 单个参数字符串最大长度（含 null 终止）。
const MAX_ARG_LEN: usize = 256;
/// execve 路径最大长度。
const MAX_PATH_LEN: usize = 128;

const ENOEXEC: isize = -8;
const EFAULT: isize = -14;
const ENOMEM: isize = -12;
const E2BIG: isize = -7;

/// 读用户 argv 指针数组（NULL 结尾），逐项拷贝字符串到内核缓冲。
fn copy_user_argv(argv_ptr: usize) -> Result<Vec<Vec<u8>>, isize> {
    let mut args = Vec::new();
    for i in 0..MAX_ARGV {
        let elem = copy_from_user(argv_ptr + i * 8, 8)?;
        let p = usize::from_ne_bytes(elem[..8].try_into().expect("8-byte usize copy"));
        if p == 0 {
            return Ok(args);
        }
        args.push(super::copy_cstring_from_user(p, MAX_ARG_LEN)?);
    }
    Err(E2BIG)
}

/// 把一段字节写入目标地址空间（按页 translate 取物理页，内核 identity 直写）。
fn write_to_user_space(space: &MemorySet, va: usize, data: &[u8]) -> Result<(), isize> {
    let mut off = 0;
    while off < data.len() {
        let addr = va + off;
        let ppn = space.translate(VirtAddr::new(addr)).ok_or(EFAULT)?;
        let dst = (ppn.value() << 12) + (addr & 0xfff);
        let n = core::cmp::min(data.len() - off, 0x1000 - (addr & 0xfff));
        // SAFETY: dst 是新地址空间已映射的用户物理页（内核 identity 映射可写），
        // 数据为内核缓冲，区间已校验。
        unsafe {
            core::ptr::copy_nonoverlapping(data.as_ptr().add(off), dst as *mut u8, n);
        }
        off += n;
    }
    Ok(())
}

/// 在新地址空间的用户栈上构建 argv：字符串从高往低、指针数组在下方，返回
/// (argc, argv 基址, 新 sp)。
fn build_user_argv_stack(
    space: &MemorySet,
    args: &[Vec<u8>],
) -> Result<(usize, usize, usize), isize> {
    let mut cursor = USER_STACK_TOP;
    let mut argv_ptrs: Vec<usize> = Vec::with_capacity(args.len());
    for arg in args {
        cursor = cursor.checked_sub(arg.len() + 1).ok_or(EFAULT)?;
        write_to_user_space(space, cursor, arg)?;
        // 字符串尾写 null 终止符。
        write_to_user_space(space, cursor + arg.len(), &[0])?;
        argv_ptrs.push(cursor);
    }
    // 指针数组 16 字节对齐（栈向下生长）。
    cursor &= !15usize;
    let array_bytes = (args.len() + 1) * core::mem::size_of::<usize>();
    cursor = cursor.checked_sub(array_bytes).ok_or(EFAULT)?;
    for (i, ptr) in argv_ptrs.iter().enumerate() {
        write_to_user_space(space, cursor + i * 8, &ptr.to_ne_bytes())?;
    }
    write_to_user_space(space, cursor + argv_ptrs.len() * 8, &[0; 8])?;
    Ok((args.len(), cursor, cursor))
}

/// execve(path, argv, envp)：envp 暂不支持（传 0 忽略）。
///
/// 成功时重写当前 TrapContext 为新程序现场并切到新地址空间，返回 Ok(())；
/// 失败返回 Err(errno)，旧进程（地址空间与现场）原样保留。
pub fn sys_execve(context: &mut arch::trap::TrapContext, args: [usize; 6]) -> Result<(), isize> {
    let path_bytes = super::copy_cstring_from_user(args[0], MAX_PATH_LEN)?;
    // exec 与文件打开共用 VFS 路径解析；错误返回时旧地址空间保持不变。
    let elf_bytes = crate::fs::read_file(&path_bytes)?;
    let elf = elf::parse(&elf_bytes).map_err(|_| ENOEXEC)?;

    // 用户参数：argv 数组（args[1]）；envp（args[2]）忽略。
    let argv = if args[1] == 0 {
        Vec::new()
    } else {
        copy_user_argv(args[1])?
    };

    // 构建新地址空间（未发布）：任何失败都保留旧进程。
    let layout = crate::kernel::mm::kernel_space::KernelImageLayout::current();
    let memory_bytes = crate::kernel::mm::configured_memory_bytes();
    let (_, memory_end) =
        arch::mm::physical_memory_range(memory_bytes).expect("configured memory range unavailable");
    let user_stack = AddressRange::new(
        VirtAddr::new(USER_STACK_TOP - 0x1000),
        VirtAddr::new(USER_STACK_TOP),
    )
    .map_err(|_| ENOMEM)?;

    let mut space =
        MemorySet::new_user(layout, VirtAddr::new(memory_end), &elf.segments, user_stack)
            .map_err(|_| ENOMEM)?;
    space
        .load_elf(&elf.segments, &elf_bytes)
        .map_err(|_| ENOMEM)?;

    // 在新栈上构建 argv；失败同样保留旧进程。
    let (argc, argv_base, new_sp) = build_user_argv_stack(&space, &argv)?;

    // 一次性替换：先切 satp 到新空间（新页表含内核身份映射，切换后当前
    // 执行安全），再替换 TCB.space（旧空间 drop，物理页释放——硬件已用新页表）。
    unsafe { arch::mm::activate(space.root_ppn().value()) };
    crate::kernel::task::exec_replace_space(space);

    // 重写现场：SPP=0（U 模式）、sepc=新入口、a0=argc、a1=argv、sp=新栈。
    *context = arch::trap::TrapContext::new_user_with_args(elf.entry, new_sp, argc, argv_base);
    Ok(())
}
