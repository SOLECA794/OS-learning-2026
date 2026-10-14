#![no_std]
#![no_main]

//! G5 小 shell（V2 §9 基础 shell 范围）：列目录 / 读文件 / 启动程序 / 等待退出。
//!
//! 自动测试用固定输入脚本 /cmds.txt（ramfs 预置）；每行一条命令，逐行执行：
//!   ls            —— openat("/") + getdents64 循环列文件名
//!   cat <path>    —— openat + read 循环到 EOF 原样输出
//!   run <path>    —— fork，子进程 execve，父进程 wait4 打印退出码
//! 未知命令打印 `sh: unknown: <cmd>`。
//!
//! 控制台 read 已独立验收；本 shell 仍以固定脚本驱动，尚无交互式行编辑；
//! 不宣称兼容 BusyBox（V2 §9：不凭提示符宣称兼容）。

use core::arch::asm;

const SYS_WRITE: usize = 64;
const SYS_READ: usize = 63;
const SYS_CLOSE: usize = 57;
const SYS_OPENAT: usize = 56;
const AT_FDCWD: usize = (-100isize) as usize;
const SYS_FORK: usize = 220;
const SYS_WAIT4: usize = 260;
const SYS_EXECVE: usize = 221;
const SYS_GETDENTS64: usize = 217;
const SYS_EXIT: usize = 93;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
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

fn write_bytes(s: &[u8]) {
    if !s.is_empty() {
        syscall(SYS_WRITE, [1, s.as_ptr() as usize, s.len(), 0, 0, 0]);
    }
}

/// 路径（无 null）复制到 null 结尾缓冲区；超长截断。
fn path_buf(path: &[u8]) -> [u8; 128] {
    let mut p = [0u8; 128];
    let n = path.len().min(127);
    p[..n].copy_from_slice(&path[..n]);
    p
}

/// 十进制打印退出码。
fn write_code(code: usize) {
    let mut tmp = [0u8; 12];
    let mut v = code;
    let mut i = tmp.len();
    if v == 0 {
        write_bytes(b"0");
        return;
    }
    while v > 0 {
        i -= 1;
        tmp[i] = b'0' + (v % 10) as u8;
        v /= 10;
    }
    write_bytes(&tmp[i..]);
}

fn cmd_ls() {
    let fd = syscall(SYS_OPENAT, [AT_FDCWD, b"/\0".as_ptr() as usize, 0, 0, 0, 0]);
    if fd < 0 {
        write_bytes(b"ls: open failed\n");
        return;
    }
    let mut buf = [0u8; 512];
    loop {
        let n = syscall(
            SYS_GETDENTS64,
            [fd as usize, buf.as_mut_ptr() as usize, 512, 0, 0, 0],
        );
        if n <= 0 {
            break;
        }
        let mut off = 0usize;
        while off < n as usize {
            let reclen = u16::from_ne_bytes([buf[off + 16], buf[off + 17]]) as usize;
            if reclen == 0 {
                break;
            }
            let mut nl = 0usize;
            while buf[off + 19 + nl] != 0 {
                nl += 1;
            }
            write_bytes(&buf[off + 19..off + 19 + nl]);
            write_bytes(b"\n");
            off += reclen;
        }
    }
    syscall(SYS_CLOSE, [fd as usize, 0, 0, 0, 0, 0]);
}

fn cmd_cat(path: &[u8]) {
    let p = path_buf(path);
    let fd = syscall(SYS_OPENAT, [AT_FDCWD, p.as_ptr() as usize, 0, 0, 0, 0]);
    if fd < 0 {
        write_bytes(b"cat: open failed\n");
        return;
    }
    let mut buf = [0u8; 64];
    loop {
        let n = syscall(
            SYS_READ,
            [fd as usize, buf.as_mut_ptr() as usize, 64, 0, 0, 0],
        );
        if n <= 0 {
            break;
        }
        syscall(SYS_WRITE, [1, buf.as_ptr() as usize, n as usize, 0, 0, 0]);
    }
    syscall(SYS_CLOSE, [fd as usize, 0, 0, 0, 0, 0]);
}

fn cmd_run(path: &[u8]) {
    let p = path_buf(path);
    let pid = syscall(SYS_FORK, [0; 6]);
    if pid < 0 {
        write_bytes(b"run: fork failed\n");
        return;
    }
    if pid == 0 {
        // 子进程：execve 目标程序（argv = [path, NULL]）。
        let argv = [p.as_ptr() as usize, 0usize];
        syscall(
            SYS_EXECVE,
            [p.as_ptr() as usize, argv.as_ptr() as usize, 0, 0, 0, 0],
        );
        write_bytes(b"run: exec failed\n");
        syscall(SYS_EXIT, [127, 0, 0, 0, 0, 0]);
        loop {}
    }
    let mut status: i32 = 0;
    let r = syscall(
        SYS_WAIT4,
        [pid as usize, &mut status as *mut i32 as usize, 0, 0, 0, 0],
    );
    if r == pid {
        write_bytes(b"[exit] code=");
        write_code(((status >> 8) & 0xff) as usize);
        write_bytes(b"\n");
    } else {
        write_bytes(b"run: wait failed\n");
    }
}

fn exec_cmd(line: &[u8]) {
    // 去首尾空白。
    let mut start = 0;
    let mut end = line.len();
    while start < end && (line[start] == b' ' || line[start] == b'\t' || line[start] == b'\r') {
        start += 1;
    }
    while end > start && (line[end - 1] == b' ' || line[end - 1] == b'\t' || line[end - 1] == b'\r')
    {
        end -= 1;
    }
    let cmd = &line[start..end];
    if cmd.is_empty() {
        return;
    }
    if cmd == b"ls" {
        cmd_ls();
    } else if cmd.starts_with(b"cat ") {
        cmd_cat(&cmd[4..]);
    } else if cmd.starts_with(b"run ") {
        cmd_run(&cmd[4..]);
    } else {
        write_bytes(b"sh: unknown: ");
        write_bytes(cmd);
        write_bytes(b"\n");
    }
}

/// 读取 /cmds.txt 全文并逐行执行（固定输入脚本，V2 §9 自动测试要求）。
#[no_mangle]
extern "C" fn _start() -> ! {
    let path = b"/cmds.txt\0";
    let fd = syscall(SYS_OPENAT, [AT_FDCWD, path.as_ptr() as usize, 0, 0, 0, 0]);
    if fd < 0 {
        write_bytes(b"shell: /cmds.txt not found\n");
        syscall(SYS_EXIT, [1, 0, 0, 0, 0, 0]);
        loop {}
    }
    // 读全量（ramfs 文件都很小，单次 1024 缓冲 + 循环）。
    let mut buf = [0u8; 1024];
    let mut total = 0usize;
    loop {
        let n = syscall(
            SYS_READ,
            [
                fd as usize,
                buf[total..].as_mut_ptr() as usize,
                buf.len() - total,
                0,
                0,
                0,
            ],
        );
        if n <= 0 {
            break;
        }
        total += n as usize;
        if total == buf.len() {
            break;
        }
    }
    syscall(SYS_CLOSE, [fd as usize, 0, 0, 0, 0, 0]);

    // 按行执行。
    let mut line_start = 0usize;
    for i in 0..total {
        if buf[i] == b'\n' {
            write_bytes(b"S$ ");
            exec_cmd(&buf[line_start..i]);
            line_start = i + 1;
        }
    }
    if line_start < total {
        write_bytes(b"S$ ");
        exec_cmd(&buf[line_start..total]);
    }
    write_bytes(b"shell: done\n");
    syscall(SYS_EXIT, [0, 0, 0, 0, 0, 0]);
    loop {}
}
