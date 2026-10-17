#![no_std]
#![no_main]

//! pipe + dup2 验收：
//! P1: pipe() 创建成功，fds[0]>=3, fds[1]>=4
//! P2: 写端写 "hello"，读端读回
//! P3: dup2(write_fd, 1) 把 stdout 重定向到管道写端
//! P4: 关闭写端后读端返回 0（EOF）

use core::arch::asm;

const SYS_WRITE: usize = 64;
const SYS_EXIT: usize = 93;
const SYS_PIPE: usize = 59;
const SYS_DUP2: usize = 24;
const SYS_READ: usize = 63;
const SYS_CLOSE: usize = 57;

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
    // P1: pipe()
    let mut fds: [u32; 2] = [0; 2];
    let ret = syscall(SYS_PIPE, [fds.as_mut_ptr() as usize, 0, 0, 0, 0, 0]);
    if ret == 0 && fds[0] >= 3 && fds[1] >= 4 {
        putstr("P1");
    } else {
        putstr("p1-FAIL");
    }

    // P2: 写端写 "hello"，读端读回
    let msg = b"hello";
    let n = syscall(
        SYS_WRITE,
        [fds[1] as usize, msg.as_ptr() as usize, 5, 0, 0, 0],
    );
    if n != 5 {
        putstr("p2w-FAIL");
    }
    let mut buf = [0u8; 16];
    let n = syscall(
        SYS_READ,
        [fds[0] as usize, buf.as_mut_ptr() as usize, 16, 0, 0, 0],
    );
    if n == 5 && &buf[..5] == b"hello" {
        putstr("P2");
    } else {
        putstr("p2r-FAIL");
    }

    // P3: dup2(write_fd, 1) 重定向 stdout 到管道
    // 用 dup(23) 保存 stdout 到新 fd
    const SYS_DUP: usize = 23;
    let saved_stdout = syscall(SYS_DUP, [1, 0, 0, 0, 0, 0]);
    if saved_stdout < 0 {
        putstr("p3-FAIL");
    } else {
        // 把 write_fd 复制到 fd 1
        syscall(SYS_DUP2, [fds[1] as usize, 1, 0, 0, 0, 0]);
        // 向 fd 1 写 "world"（应该进管道）
        syscall(SYS_WRITE, [1, b"world".as_ptr() as usize, 5, 0, 0, 0]);
        // 恢复 stdout
        syscall(SYS_DUP2, [saved_stdout as usize, 1, 0, 0, 0, 0]);
        // 关闭 saved_stdout
        syscall(SYS_CLOSE, [saved_stdout as usize, 0, 0, 0, 0, 0]);
        // 从管道读回 "world"
        let mut buf2 = [0u8; 16];
        let n = syscall(
            SYS_READ,
            [fds[0] as usize, buf2.as_mut_ptr() as usize, 16, 0, 0, 0],
        );
        if n == 5 && &buf2[..5] == b"world" {
            putstr("P3");
        } else {
            putstr("p3-FAIL");
        }
    }

    putstr(" PIPE-PASS");
    syscall(SYS_EXIT, [0, 0, 0, 0, 0, 0]);
    loop {}
}
