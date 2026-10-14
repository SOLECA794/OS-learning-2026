#![no_std]
#![no_main]

//! M2.5 控制台输入验收：读者阻塞时，另一个就绪任务仍能运行。

use core::arch::asm;

const SYS_WRITE: usize = 64;
const SYS_READ: usize = 63;
const SYS_FORK: usize = 220;
const SYS_WAIT4: usize = 260;
const SYS_EXIT: usize = 93;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

fn syscall(id: usize, args: [usize; 6]) -> isize {
    let ret: isize;
    unsafe {
        asm!(
            "ecall",
            inlateout("x10") args[0] => ret,
            in("x11") args[1], in("x12") args[2], in("x13") args[3],
            in("x14") args[4], in("x15") args[5], in("x17") id,
            options(nostack),
        );
    }
    ret
}

fn write(bytes: &[u8]) {
    syscall(
        SYS_WRITE,
        [1, bytes.as_ptr() as usize, bytes.len(), 0, 0, 0],
    );
}

fn fail() -> ! {
    write(b"INPUT_FAIL\n");
    syscall(SYS_EXIT, [1, 0, 0, 0, 0, 0]);
    loop {}
}

#[no_mangle]
extern "C" fn _start() -> ! {
    let mut buf = [0u8; 8];
    // 零长度不应等待，非法 fd 仍须返回 EBADF。
    if syscall(SYS_READ, [0, buf.as_mut_ptr() as usize, 0, 0, 0, 0]) != 0
        || syscall(SYS_READ, [1, buf.as_mut_ptr() as usize, 1, 0, 0, 0]) != 0
        || syscall(SYS_READ, [99, buf.as_mut_ptr() as usize, 1, 0, 0, 0]) != -9
        || syscall(SYS_READ, [0, 0x3000, 1, 0, 0, 0]) != -14
    {
        fail();
    }
    let reader = syscall(SYS_FORK, [0; 6]);
    if reader < 0 {
        fail();
    }
    if reader == 0 {
        read_console();
        syscall(SYS_EXIT, [7, 0, 0, 0, 0, 0]);
        loop {}
    }
    let worker = syscall(SYS_FORK, [0; 6]);
    if worker < 0 {
        fail();
    }
    if worker == 0 {
        // 运行器只在观察到此标记后注入输入；日志须先记录读者等待。
        write(b"INPUT_OTHER_TASK\n");
        syscall(SYS_EXIT, [8, 0, 0, 0, 0, 0]);
        loop {}
    }
    let mut status: i32 = 0;
    if syscall(
        SYS_WAIT4,
        [
            reader as usize,
            &mut status as *mut i32 as usize,
            0,
            0,
            0,
            0,
        ],
    ) != reader
        || ((status >> 8) & 0xff) != 7
    {
        fail();
    }
    if syscall(
        SYS_WAIT4,
        [
            worker as usize,
            &mut status as *mut i32 as usize,
            0,
            0,
            0,
            0,
        ],
    ) != worker
        || ((status >> 8) & 0xff) != 8
    {
        fail();
    }
    write(b"INPUT_PASS\n");
    syscall(SYS_EXIT, [0, 0, 0, 0, 0, 0]);
    loop {}
}

fn read_console() {
    let mut buf = [0u8; 8];
    write(b"INPUT_WAIT_READY\n");
    // fixture 为 abc\nxyz\n，允许 UART 分批到达、read 合法短读。
    let mut total = 0usize;
    while total < 4 {
        let n = syscall(
            SYS_READ,
            [0, buf[total..].as_mut_ptr() as usize, 4 - total, 0, 0, 0],
        );
        if n <= 0 {
            fail();
        }
        total += n as usize;
    }
    if &buf[..4] != b"abc\n" {
        fail();
    }
    write(b"INPUT_FIRST_DONE\n");
    // 第二行由同一批 fixture 提供；UART 可分批交付，短读或再次等待均合法。
    let mut second = [0u8; 4];
    let mut read_second = 0usize;
    while read_second < second.len() {
        let n = syscall(
            SYS_READ,
            [
                0,
                second[read_second..].as_mut_ptr() as usize,
                second.len() - read_second,
                0,
                0,
                0,
            ],
        );
        if n <= 0 {
            fail();
        }
        read_second += n as usize;
    }
    if &second != b"xyz\n" {
        fail();
    }
}
