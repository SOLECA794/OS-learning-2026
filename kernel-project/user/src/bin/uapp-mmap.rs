#![no_std]
#![no_main]

//! mmap/munmap 验收：
//! 验证匿名多页映射、部分 munmap 拆分、缺页错误和剩余区域清理。

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
    // mmap 零长度按 Linux ABI 返回 EINVAL。
    if syscall(SYS_MMAP, [0, 0, 3, 0x20, 0, 0]) != -22 {
        putstr("mmap-zero-FAIL");
        syscall(SYS_EXIT, [1, 0, 0, 0, 0, 0]);
        loop {}
    }

    // 建立三页匿名映射，分别写入首尾页作为保留映射的哨兵。
    let addr = syscall(SYS_MMAP, [0, 3 * 4096, 3, 0x20, 0, 0]);
    if addr > 0 {
    } else {
        putstr("m1-FAIL");
        syscall(SYS_EXIT, [1, 0, 0, 0, 0, 0]);
        loop {}
    }
    unsafe {
        (addr as *mut u8).write(0x42);
        ((addr + 2 * 4096) as *mut u8).write(0x24);
    }

    // 非页对齐地址和零长度都按 Linux munmap 语义拒绝，且不破坏原映射。
    if syscall(SYS_MUNMAP, [addr as usize + 1, 4096, 0, 0, 0, 0]) != -22
        || syscall(SYS_MUNMAP, [addr as usize, 0, 0, 0, 0, 0]) != -22
    {
        putstr("munmap-arguments-FAIL");
        syscall(SYS_EXIT, [1, 0, 0, 0, 0, 0]);
        loop {}
    }

    // 只解除中间页：首尾页继续可用，缺页上的 copy_from_user 返回 EFAULT。
    let ret = syscall(SYS_MUNMAP, [addr as usize + 4096, 4096, 0, 0, 0, 0]);
    let (first, last) = unsafe {
        (
            (addr as *const u8).read(),
            ((addr + 2 * 4096) as *const u8).read(),
        )
    };
    let hole = syscall(SYS_WRITE, [1, addr as usize + 4096, 1, 0, 0, 0]);
    if ret != 0 || first != 0x42 || last != 0x24 || hole != -14 {
        putstr("munmap-FAIL");
        syscall(SYS_EXIT, [1, 0, 0, 0, 0, 0]);
        loop {}
    }

    // 最后清理包含已解除洞的完整区间，剩余用户页都应回收。
    let ret = syscall(SYS_MUNMAP, [addr as usize, 3 * 4096, 0, 0, 0, 0]);
    if ret != 0 {
        putstr("munmap-cleanup-FAIL");
        syscall(SYS_EXIT, [1, 0, 0, 0, 0, 0]);
        loop {}
    }

    putstr("MMAP-PASS");
    syscall(SYS_EXIT, [0, 0, 0, 0, 0, 0]);
    loop {}
}
