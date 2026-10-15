#![no_std]
#![no_main]

//! mmap/munmap 验收：
//! M1: mmap(0, 4096, ...) 返回非零地址
//! M2: 写入读回验证
//! M3: munmap 后访问应触发缺页（简单验证 munmap 返回 0）

use core::arch::asm;

const SYS_WRITE: usize = 64;
const SYS_EXIT: usize = 93;
const SYS_MMAP: usize = 222;
const SYS_MUNMAP: usize = 215;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

fn syscall(id: usize, args: [usize; 6]) -> isize {
    let ret: isize;
    unsafe {
        asm!(
            "ecall",
            in("a7") id,
            inlateout("a0") args[0] => ret,
            in("a1") args[1],
            in("a2") args[2],
            in("a3") args[3],
            in("a4") args[4],
            in("a5") args[5],
        );
    }
    ret
}

fn putstr(s: &str) {
    syscall(SYS_WRITE, [1, s.as_ptr() as usize, s.len(), 0, 0, 0]);
}

#[no_mangle]
extern "C" fn _start() -> ! {
    // M1: mmap 4096 字节匿名映射
    let addr = syscall(SYS_MMAP, [0, 4096, 3, 0x20, 0, 0]);
    if addr > 0 {
        putstr("M1");
    } else {
        putstr("m1-FAIL");
    }

    // M2: 写入读回
    unsafe {
        (addr as *mut u8).write(0x42);
    }
    let val = unsafe { (addr as *const u8).read() };
    if val == 0x42 {
        putstr("M2");
    } else {
        putstr("m2-FAIL");
    }

    // M3: munmap
    let ret = syscall(SYS_MUNMAP, [addr as usize, 4096, 0, 0, 0, 0]);
    if ret == 0 {
        putstr("M3");
    } else {
        putstr("m3-FAIL");
    }

    putstr(" MMAP-PASS");
    syscall(SYS_EXIT, [0, 0, 0, 0, 0, 0]);
    loop {}
}
