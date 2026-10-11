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

#[no_mangle]
extern "C" fn _start() -> ! {
    let msg = b"hello from user mode\n";
    // a7=64: write(1, buf, len)
    syscall3(SYS_WRITE, [1, msg.as_ptr() as usize, msg.len()]);
    // a7=93: exit(0)，不返回
    syscall3(SYS_EXIT, [0, 0, 0]);
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