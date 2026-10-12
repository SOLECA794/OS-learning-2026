#![no_std]
#![no_main]

//! 用户态测试程序：write(1, ...)（M1.5 起）；M2 起默认模式为无主动 yield 的自旋循环。
//! 链接基址 0x10000（user/user.ld）；系统调用用 RV 内联汇编 ecall。

use core::arch::asm;

const SYS_WRITE: usize = 64;
const SYS_EXIT: usize = 93;
const SYS_FORK: usize = 220;
const SYS_WAIT4: usize = 260;
const SYS_EXECVE: usize = 221;
const SYS_OPENAT: usize = 56;
const SYS_CLOSE: usize = 57;
const SYS_READ: usize = 63;
const SYS_DUP: usize = 23;

/// G1 连续页复用验收轮数：每轮 fork 一子并 wait 回收。
/// 8 轮后内核侧帧计数应每轮对称（fork 后 / wait 后数值逐轮一致），
/// 分配器 bump 水位不应随轮次增长（空闲块复用生效）。
const ROUNDS: usize = 8;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

/// 默认模式（G4 fork 验收循环）：每轮 fork 一子（子写 1 个 c 后 exit(42)），
/// 父 wait4 校验 WEXITSTATUS==42，打印 'R'；8 轮全过后打印 PASS 并 exit(0)。
/// 时钟抢占保持开启（子/父均可能被抢占，但验收不依赖抢占时序）。
#[cfg(not(feature = "fault-test"))]
#[no_mangle]
extern "C" fn _start() -> ! {
    for _ in 0..ROUNDS {
        let pid = syscall(SYS_FORK, [0; 6]) as isize;
        if pid < 0 {
            syscall(SYS_WRITE, [1, b"FORK_FAIL\n".as_ptr() as usize, 10, 0, 0, 0]);
            syscall(SYS_EXIT, [1, 0, 0, 0, 0, 0]);
            loop {}
        }
        if pid == 0 {
            syscall(SYS_WRITE, [1, b"c".as_ptr() as usize, 1, 0, 0, 0]);
            syscall(SYS_EXIT, [42, 0, 0, 0, 0, 0]);
            loop {}
        }
        let mut status: usize = 0;
        let r = syscall(SYS_WAIT4, [pid as usize, &mut status as *mut usize as usize, 0, 0, 0, 0]);
        if r != pid || ((status >> 8) & 0xff) != 42 {
            syscall(SYS_WRITE, [1, b"FAIL\n".as_ptr() as usize, 5, 0, 0, 0]);
            syscall(SYS_EXIT, [1, 0, 0, 0, 0, 0]);
            loop {}
        }
        syscall(SYS_WRITE, [1, b"R".as_ptr() as usize, 1, 0, 0, 0]);
    }
    syscall(SYS_WRITE, [1, b"PASS\n".as_ptr() as usize, 5, 0, 0, 0]);

    // ==== G4 exec 验收 ====
    // ① 不存在路径 → -ENOENT(-2)，旧进程保留继续。
    let bad_path = b"/nonexist\0";
    let r1 = syscall(SYS_EXECVE, [bad_path.as_ptr() as usize, 0, 0, 0, 0, 0]);
    if r1 != -2 {
        syscall(SYS_WRITE, [1, b"E1_FAIL\n".as_ptr() as usize, 8, 0, 0, 0]);
        syscall(SYS_EXIT, [1, 0, 0, 0, 0, 0]);
        loop {}
    }
    syscall(SYS_WRITE, [1, b"E1".as_ptr() as usize, 2, 0, 0, 0]);

    // ② 坏 ELF（/bad）→ -ENOEXEC(-8)，旧进程保留继续。
    let bad_elf_path = b"/bad\0";
    let r2 = syscall(SYS_EXECVE, [bad_elf_path.as_ptr() as usize, 0, 0, 0, 0, 0]);
    if r2 != -8 {
        syscall(SYS_WRITE, [1, b"E2_FAIL\n".as_ptr() as usize, 8, 0, 0, 0]);
        syscall(SYS_EXIT, [1, 0, 0, 0, 0, 0]);
        loop {}
    }
    syscall(SYS_WRITE, [1, b"E2".as_ptr() as usize, 2, 0, 0, 0]);

    // ==== G5 文件验收（ramfs + FD + 共享偏移）====
    // F1：openat 已存在文件 + read 校验 + close。
    let hello = b"/hello.txt\0";
    let fd1 = syscall(SYS_OPENAT, [hello.as_ptr() as usize, 0, 0, 0, 0, 0]);
    if fd1 < 0 {
        syscall(SYS_WRITE, [1, b"F1_FAIL\n".as_ptr() as usize, 8, 0, 0, 0]);
        syscall(SYS_EXIT, [1, 0, 0, 0, 0, 0]);
        loop {}
    }
    let mut buf1 = [0u8; 32];
    let n1 = syscall(SYS_READ, [fd1 as usize, buf1.as_mut_ptr() as usize, 11, 0, 0, 0]);
    if n1 != 11 || &buf1[..11] != b"hello ramfs" {
        syscall(SYS_WRITE, [1, b"F1_FAIL\n".as_ptr() as usize, 8, 0, 0, 0]);
        syscall(SYS_EXIT, [1, 0, 0, 0, 0, 0]);
        loop {}
    }
    syscall(SYS_CLOSE, [fd1 as usize, 0, 0, 0, 0, 0]);
    syscall(SYS_WRITE, [1, b"F1".as_ptr() as usize, 2, 0, 0, 0]);

    // F2：不存在文件 → -ENOENT。
    let nofile = b"/nofile\0";
    let r2 = syscall(SYS_OPENAT, [nofile.as_ptr() as usize, 0, 0, 0, 0, 0]);
    if r2 != -2 {
        syscall(SYS_WRITE, [1, b"F2_FAIL\n".as_ptr() as usize, 8, 0, 0, 0]);
        syscall(SYS_EXIT, [1, 0, 0, 0, 0, 0]);
        loop {}
    }
    syscall(SYS_WRITE, [1, b"F2".as_ptr() as usize, 2, 0, 0, 0]);

    // F3：O_CREAT 创建 + 写 + 重开读回。
    let newfile = b"/new.txt\0";
    let fd3 = syscall(SYS_OPENAT, [newfile.as_ptr() as usize, 2 | 0x40, 0, 0, 0, 0]);
    if fd3 < 0 {
        syscall(SYS_WRITE, [1, b"F3_FAIL\n".as_ptr() as usize, 8, 0, 0, 0]);
        syscall(SYS_EXIT, [1, 0, 0, 0, 0, 0]);
        loop {}
    }
    let data3 = b"abcde";
    let w3 = syscall(SYS_WRITE, [fd3 as usize, data3.as_ptr() as usize, 5, 0, 0, 0]);
    if w3 != 5 {
        syscall(SYS_WRITE, [1, b"F3_FAIL\n".as_ptr() as usize, 8, 0, 0, 0]);
        syscall(SYS_EXIT, [1, 0, 0, 0, 0, 0]);
        loop {}
    }
    syscall(SYS_CLOSE, [fd3 as usize, 0, 0, 0, 0, 0]);
    let fd3b = syscall(SYS_OPENAT, [newfile.as_ptr() as usize, 0, 0, 0, 0, 0]);
    let mut buf3 = [0u8; 8];
    let n3 = syscall(SYS_READ, [fd3b as usize, buf3.as_mut_ptr() as usize, 5, 0, 0, 0]);
    if n3 != 5 || &buf3[..5] != b"abcde" {
        syscall(SYS_WRITE, [1, b"F3_FAIL\n".as_ptr() as usize, 8, 0, 0, 0]);
        syscall(SYS_EXIT, [1, 0, 0, 0, 0, 0]);
        loop {}
    }
    syscall(SYS_CLOSE, [fd3b as usize, 0, 0, 0, 0, 0]);
    syscall(SYS_WRITE, [1, b"F3".as_ptr() as usize, 2, 0, 0, 0]);

    // F4：非法 fd → EBADF(-9)。
    let r4a = syscall(SYS_READ, [99, 0, 0, 0, 0, 0]);
    let r4b = syscall(SYS_WRITE, [99, 0, 0, 0, 0, 0]);
    let r4c = syscall(SYS_CLOSE, [99, 0, 0, 0, 0, 0]);
    if r4a != -9 || r4b != -9 || r4c != -9 {
        syscall(SYS_WRITE, [1, b"F4_FAIL\n".as_ptr() as usize, 8, 0, 0, 0]);
        syscall(SYS_EXIT, [1, 0, 0, 0, 0, 0]);
        loop {}
    }
    syscall(SYS_WRITE, [1, b"F4".as_ptr() as usize, 2, 0, 0, 0]);

    // F5：dup 共享偏移——原 fd 读 2 后，dup fd 从同一偏移继续读。
    let dupfile = b"/dup.txt\0";
    let fd5 = syscall(SYS_OPENAT, [dupfile.as_ptr() as usize, 2, 0, 0, 0, 0]);
    if fd5 < 0 {
        syscall(SYS_WRITE, [1, b"F5_FAIL\n".as_ptr() as usize, 8, 0, 0, 0]);
        syscall(SYS_EXIT, [1, 0, 0, 0, 0, 0]);
        loop {}
    }
    let mut b5a = [0u8; 2];
    let n5 = syscall(SYS_READ, [fd5 as usize, b5a.as_mut_ptr() as usize, 2, 0, 0, 0]);
    if n5 != 2 || b5a != *b"ab" {
        syscall(SYS_WRITE, [1, b"F5_FAIL\n".as_ptr() as usize, 8, 0, 0, 0]);
        syscall(SYS_EXIT, [1, 0, 0, 0, 0, 0]);
        loop {}
    }
    let fd5b = syscall(SYS_DUP, [fd5 as usize, 0, 0, 0, 0, 0]);
    if fd5b < 0 {
        syscall(SYS_WRITE, [1, b"F5_FAIL\n".as_ptr() as usize, 8, 0, 0, 0]);
        syscall(SYS_EXIT, [1, 0, 0, 0, 0, 0]);
        loop {}
    }
    let mut b5b = [0u8; 4];
    let n5b = syscall(SYS_READ, [fd5b as usize, b5b.as_mut_ptr() as usize, 4, 0, 0, 0]);
    if n5b != 4 || b5b != *b"cdef" {
        syscall(SYS_WRITE, [1, b"F5_FAIL\n".as_ptr() as usize, 8, 0, 0, 0]);
        syscall(SYS_EXIT, [1, 0, 0, 0, 0, 0]);
        loop {}
    }
    syscall(SYS_CLOSE, [fd5 as usize, 0, 0, 0, 0, 0]);
    syscall(SYS_CLOSE, [fd5b as usize, 0, 0, 0, 0, 0]);
    syscall(SYS_WRITE, [1, b"F5".as_ptr() as usize, 2, 0, 0, 0]);

    // F6：fork 共享偏移——父子各读 2 字节，合计恰好覆盖 {ab, cd}（顺序无关验收）。
    let forkfile = b"/fork.txt\0";
    let fd6 = syscall(SYS_OPENAT, [forkfile.as_ptr() as usize, 2, 0, 0, 0, 0]);
    if fd6 < 0 {
        syscall(SYS_WRITE, [1, b"F6_FAIL\n".as_ptr() as usize, 8, 0, 0, 0]);
        syscall(SYS_EXIT, [1, 0, 0, 0, 0, 0]);
        loop {}
    }
    let pid6 = syscall(SYS_FORK, [0; 6]);
    if pid6 < 0 {
        syscall(SYS_WRITE, [1, b"F6_FAIL\n".as_ptr() as usize, 8, 0, 0, 0]);
        syscall(SYS_EXIT, [1, 0, 0, 0, 0, 0]);
        loop {}
    }
    if pid6 == 0 {
        // 子：读 2 字节，exit code = 首字节（'a'=97 / 'c'=99）。
        let mut cb = [0u8; 2];
        syscall(SYS_READ, [fd6 as usize, cb.as_mut_ptr() as usize, 2, 0, 0, 0]);
        syscall(SYS_EXIT, [cb[0] as usize, 0, 0, 0, 0, 0]);
        loop {}
    }
    let mut pb = [0u8; 2];
    let n6 = syscall(SYS_READ, [fd6 as usize, pb.as_mut_ptr() as usize, 2, 0, 0, 0]);
    if n6 != 2 {
        syscall(SYS_WRITE, [1, b"F6_FAIL\n".as_ptr() as usize, 8, 0, 0, 0]);
        syscall(SYS_EXIT, [1, 0, 0, 0, 0, 0]);
        loop {}
    }
    let mut status6: usize = 0;
    let r6 = syscall(SYS_WAIT4, [pid6 as usize, &mut status6 as *mut usize as usize, 0, 0, 0, 0]);
    if r6 != pid6 {
        syscall(SYS_WRITE, [1, b"F6_FAIL\n".as_ptr() as usize, 8, 0, 0, 0]);
        syscall(SYS_EXIT, [1, 0, 0, 0, 0, 0]);
        loop {}
    }
    let child_first = ((status6 >> 8) & 0xff) as u8;
    let ok6 = (pb[0] == b'a' && child_first == b'c') || (pb[0] == b'c' && child_first == b'a');
    if !ok6 {
        syscall(SYS_WRITE, [1, b"F6_FAIL\n".as_ptr() as usize, 8, 0, 0, 0]);
        syscall(SYS_EXIT, [1, 0, 0, 0, 0, 0]);
        loop {}
    }
    syscall(SYS_CLOSE, [fd6 as usize, 0, 0, 0, 0, 0]);
    syscall(SYS_WRITE, [1, b"F6".as_ptr() as usize, 2, 0, 0, 0]);
    // ③ 成功 exec：/uapp2 带 argv；成功后现场被替换，本函数不应继续。
    let path = b"/uapp2\0";
    let arg1 = b"hello\0";
    let argv = [path.as_ptr() as usize, arg1.as_ptr() as usize, 0usize];
    syscall(
        SYS_EXECVE,
        [path.as_ptr() as usize, argv.as_ptr() as usize, 0, 0, 0, 0],
    );
    // 若到达：exec 未换现场（实现回归）。
    syscall(SYS_WRITE, [1, b"EXEC_FAIL\n".as_ptr() as usize, 10, 0, 0, 0]);
    syscall(SYS_EXIT, [1, 0, 0, 0, 0, 0]);
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
