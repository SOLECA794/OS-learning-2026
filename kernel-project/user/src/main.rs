#![no_std]
#![no_main]

//! 用户态测试程序：write(1, ...)（M1.5 起）；M2 起默认模式为无主动 yield 的自旋循环。
//! 链接基址 0x10000（user/user.ld）；系统调用用 RV 内联汇编 ecall。

use core::arch::asm;

const SYS_WRITE: usize = 64;
const SYS_EXIT: usize = 93;
const SYS_FORK: usize = 220;
const SYS_WAIT4: usize = 260;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

/// 默认模式（M3 fork 验收）：子写 3 次后 exit(42)；父 wait4 校验 WEXITSTATUS 后 exit(0)。
/// 时钟抢占保持开启（子/父均可能被抢占，但验收不依赖抢占时序）。
#[cfg(not(feature = "fault-test"))]
#[no_mangle]
extern "C" fn _start() -> ! {
    let pid = syscall(SYS_FORK, [0; 6]) as isize;
    if pid < 0 {
        syscall(SYS_EXIT, [1, 0, 0, 0, 0, 0]);
        loop {}
    }
    if pid == 0 {
        // 子：写 3 个 c 后以 42 退出（wait status 低 8 位）。
        for _ in 0..3 {
            syscall(SYS_WRITE, [1, b"c".as_ptr() as usize, 1, 0, 0, 0]);
        }
        syscall(SYS_EXIT, [42, 0, 0, 0, 0, 0]);
        loop {}
    }
    // 父：wait4(pid, &status, 0, 0)；校验返回 pid 且 WEXITSTATUS == 42。
    let mut status: usize = 0;
    let r = syscall(SYS_WAIT4, [pid as usize, &mut status as *mut usize as usize, 0, 0, 0, 0]);
    if r == pid && ((status >> 8) & 0xff) == 42 {
        syscall(SYS_WRITE, [1, b"PASS\n".as_ptr() as usize, 5, 0, 0, 0]);
        syscall(SYS_EXIT, [0, 0, 0, 0, 0, 0]);
    } else {
        syscall(SYS_WRITE, [1, b"FAIL\n".as_ptr() as usize, 5, 0, 0, 0]);
        syscall(SYS_EXIT, [1, 0, 0, 0, 0, 0]);
    }
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
    syscall(SYS_EXIT, [2, 0, 0, 0, 0, 0]);
    loop {}
}

#[inline(never)]
fn syscall(id: usize, args: [usize; 6]) -> isize {
    let ret: isize;
    unsafe {
        asm!(
            "ecall",
            inlateout("x10") args[0] => ret,
            in("x11") args[1],
            in("x12") args[2],
            in("x13") args[3],
            in("x14") args[4],
            in("x15") args[5],
            in("x17") id,
            options(nostack),
        );
    }
    ret
}
