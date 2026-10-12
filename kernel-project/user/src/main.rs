#![no_std]
#![no_main]

//! M1.5 最小用户程序：write(1, ...) 后 exit(0)。
//! 链接基址 0x10000（user/user.ld）；系统调用用 RV 内联汇编 ecall。

use core::arch::asm;

const SYS_WRITE: usize = 64;
const SYS_EXIT: usize = 93;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

/// 默认模式（W2 附加用例）：三连测后按结果退出。
///
/// 校验 syscall 错误语义（规划书 W2 验收：非法指针受控、错误 fd 受控）：
///   1. write(1, msg, 21)        → 期望 21（完整写入，msg 真实长度 21）
///   2. write(1, NULL, 5)        → 期望 -14（EFAULT：0 页未映射，低于用户区下界）
///   3. write(2, msg, 21)        → 期望 -9（EBADF：当前仅支持 fd=1）
/// 全部符合 → exit(0)；否则 exit(1)，退出码可被内核日志观测。
#[cfg(not(feature = "fault-test"))]
#[no_mangle]
extern "C" fn _start() -> ! {
    let msg = b"hello from user mode\n";
    let r1 = syscall3(SYS_WRITE, [1, msg.as_ptr() as usize, msg.len()]);
    let r2 = syscall3(SYS_WRITE, [1, 0, 5]);
    let r3 = syscall3(SYS_WRITE, [2, msg.as_ptr() as usize, msg.len()]);
    let ok = r1 == 21 && r2 == -14 && r3 == -9;
    syscall3(SYS_EXIT, [if ok { 0 } else { 1 }, 0, 0]);
    loop {}
}

/// fault-test 模式（W2 附加用例②）：用户态读取内核保护区 0x80200000。
///
/// 该页在内核映射中存在但无 USER 位（S-only），U 模式访问 → 页错误 → 内核 fatal trap
/// （打印 pc/bad_addr/from_user=true 后受控停机）。若意外未触发（实现回归），
/// exit(2) 让退出码在日志中可观测。
#[cfg(feature = "fault-test")]
#[no_mangle]
extern "C" fn _start() -> ! {
    let ptr = 0x8020_0000usize as *const u8;
    let _v = core::hint::black_box(unsafe { *ptr });
    syscall3(SYS_EXIT, [2, 0, 0]);
    loop {}
}

#[inline(never)]
fn syscall3(id: usize, args: [usize; 3]) -> isize {
    let ret: isize;
    unsafe {
        asm!(
            "ecall",
            inlateout("x10") args[0] => ret,
            in("x11") args[1],
            in("x12") args[2],
            in("x17") id,
            options(nostack),
        );
    }
    ret
}