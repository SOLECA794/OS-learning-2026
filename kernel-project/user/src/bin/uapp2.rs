#![no_std]
#![no_main]

//! exec 目标程序（G4 验收）：uapp exec("/uapp2", [..]) 后本程序运行。
//! 输出 'X'（证明 exec 换程序成功）后以 7 退出（内核日志可见 exit code）。

use core::arch::asm;

const SYS_WRITE: usize = 64;
const SYS_EXIT: usize = 93;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

/// exec 成功后从新现场进入：a0=argc、a1=argv（C ABI）。
#[no_mangle]
extern "C" fn _start(_argc: usize, _argv: usize) -> ! {
    syscall(SYS_WRITE, [1, b"X".as_ptr() as usize, 1, 0, 0, 0]);
    syscall(SYS_EXIT, [7, 0, 0, 0, 0, 0]);
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
