#![no_std]
#![no_main]

//! 用户态测试程序：write(1, ...)（M1.5 起）；M2 起默认模式为无主动 yield 的自旋循环。
//! 链接基址 0x10000（user/user.ld）；系统调用用 RV 内联汇编 ecall。

use core::arch::asm;

const SYS_WRITE: usize = 64;
const SYS_EXIT: usize = 93;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

/// 默认模式（M2 调度验收）：自旋循环 + 周期写，不主动 yield。
///
///
/// 时钟抢占驱动两个任务交替运行；观测点 = 每 500_000 次自旋写一次 tick。
#[cfg(not(feature = "fault-test"))]
#[no_mangle]
extern "C" fn _start() -> ! {
    // M2 调度验收程序：无主动 yield 的自旋循环，每 500_000 次写一次 tick。
    // 内核时钟抢占驱动两个任务交替运行；观测点 = 每个任务的 tick 持续推进。
    let msg = b"tick\n";
    let mut counter: usize = 0;
    loop {
        counter += 1;
        if counter % 500_000 == 0 {
            syscall3(SYS_WRITE, [1, msg.as_ptr() as usize, msg.len()]);
            counter = 0;
        }
    }
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