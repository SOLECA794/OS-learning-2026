#![no_std]
#![no_main]

//! M2.5 VFS 用户态验收：真实 syscall 创建目录、操作 FD，再执行子目录 ELF。
//! 每个 PASS 只在对应断言完成后输出；子进程错误必须通过 wait4 状态传回父进程。

use core::arch::asm;

const DUP: usize = 23;
const MKDIRAT: usize = 34;
const OPENAT: usize = 56;
const LSEEK: usize = 62;
const CLOSE: usize = 57;
const READ: usize = 63;
const PREAD64: usize = 67;
const PWRITE64: usize = 68;
const WRITE: usize = 64;
const FSTAT: usize = 80;
const EXIT: usize = 93;
const GETDENTS: usize = 217;
const FORK: usize = 220;
const EXEC: usize = 221;
const WAIT: usize = 260;
const CREATE: usize = 0x40;
const EXCL: usize = 0x80;
const TRUNC: usize = 0x200;
const APPEND: usize = 0x400;
const DIRECTORY: usize = 0x10000;
const AT_FDCWD: usize = (-100isize) as usize;

/// Linux RV64 stat 恰好 128 字节；尾部 canary 证明内核不会多写。
#[repr(C)]
struct StatSlot {
    bytes: [u8; 128],
    canary: u32,
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    fail(b"panic");
}

fn syscall(id: usize, args: [usize; 6]) -> isize {
    let ret: isize;
    unsafe {
        asm!("ecall", inlateout("a0") args[0] => ret, in("a1") args[1],
            in("a2") args[2], in("a3") args[3], in("a4") args[4], in("a5") args[5],
            in("a7") id, options(nostack));
    }
    ret
}

fn write(fd: isize, bytes: &[u8]) -> isize {
    syscall(
        WRITE,
        [fd as usize, bytes.as_ptr() as usize, bytes.len(), 0, 0, 0],
    )
}
fn read(fd: isize, bytes: &mut [u8]) -> isize {
    syscall(
        READ,
        [
            fd as usize,
            bytes.as_mut_ptr() as usize,
            bytes.len(),
            0,
            0,
            0,
        ],
    )
}
fn open(path: &[u8], flags: usize) -> isize {
    open_at(AT_FDCWD, path, flags, 0)
}
fn open_at(dirfd: usize, path: &[u8], flags: usize, mode: usize) -> isize {
    syscall(OPENAT, [dirfd, path.as_ptr() as usize, flags, mode, 0, 0])
}
fn mkdir(path: &[u8]) -> isize {
    mkdir_at(AT_FDCWD, path)
}
fn mkdir_at(dirfd: usize, path: &[u8]) -> isize {
    syscall(
        MKDIRAT,
        [dirfd, path.as_ptr() as usize, 0o755, 0, 0, 0],
    )
}
fn seek(fd: isize, offset: isize, whence: usize) -> isize {
    syscall(LSEEK, [fd as usize, offset as usize, whence, 0, 0, 0])
}
fn pread(fd: isize, bytes: &mut [u8], offset: isize) -> isize {
    syscall(
        PREAD64,
        [fd as usize, bytes.as_mut_ptr() as usize, bytes.len(), offset as usize, 0, 0],
    )
}
fn pwrite(fd: isize, bytes: &[u8], offset: isize) -> isize {
    syscall(
        PWRITE64,
        [fd as usize, bytes.as_ptr() as usize, bytes.len(), offset as usize, 0, 0],
    )
}
fn close(fd: isize) {
    check(syscall(CLOSE, [fd as usize, 0, 0, 0, 0, 0]) == 0, b"close");
}
fn exit(code: usize) -> ! {
    syscall(EXIT, [code, 0, 0, 0, 0, 0]);
    loop {}
}
fn fail(case: &[u8]) -> ! {
    write(1, b"VFS_FAIL:");
    write(1, case);
    write(1, b"\n");
    exit(1)
}
fn check(ok: bool, case: &[u8]) {
    if !ok {
        fail(case);
    }
}
fn opened(path: &[u8], flags: usize) -> isize {
    let fd = open(path, flags);
    check(fd >= 0, path);
    fd
}
fn wait(pid: isize, code: i32) {
    let mut status: i32 = 0;
    check(
        syscall(
            WAIT,
            [pid as usize, &mut status as *mut i32 as usize, 0, 0, 0, 0],
        ) == pid
            && ((status >> 8) & 0xff) == code,
        b"wait status",
    );
}

/// 一个记录正好 32 字节，检查 NUL、类型、inode 和下次枚举游标。
fn dirent(fd: isize, name: &[u8], kind: u8, next: usize) -> u64 {
    let mut record = [0xffu8; 32];
    check(
        syscall(
            GETDENTS,
            [fd as usize, record.as_mut_ptr() as usize, 32, 0, 0, 0],
        ) == 32,
        b"dirent length",
    );
    check(
        record[16..18] == 32u16.to_ne_bytes() && record[18] == kind,
        b"dirent type",
    );
    check(
        &record[19..19 + name.len()] == name && record[19 + name.len()] == 0,
        b"dirent name NUL",
    );
    check(
        u64::from_ne_bytes(record[8..16].try_into().unwrap()) == next as u64,
        b"dirent offset",
    );
    let ino = u64::from_ne_bytes(record[..8].try_into().unwrap());
    check(ino != 0, b"inode zero");
    ino
}

#[no_mangle]
extern "C" fn _start() -> ! {
    // 第二后端经同一 open/read/目录 FD 路径可达；写入与创建须明确失败。
    let static_dir = opened(b"/static\0", DIRECTORY);
    let static_file = open_at(static_dir as usize, b"readme\0", 0, 0);
    check(static_file >= 0, b"static dirfd open");
    let mut static_bytes = [0u8; 17];
    check(
        read(static_file, &mut static_bytes) == 17
            && static_bytes == *b"staticfs via vfs\n",
        b"static backend read",
    );
    close(static_file);
    check(
        open(b"/static/missing\0", 2 | CREATE) == -30,
        b"static backend readonly",
    );
    check(dirent(static_dir, b"readme", 8, 1) != 0, b"static dirent");
    close(static_dir);
    write(1, b"VFS_STATIC_PASS\n");

    // 路径逐段解析，失败不能创建错误节点。
    check(
        mkdir(b"/vfs\0") == 0 && mkdir(b"/vfs/sub\0") == 0,
        b"mkdir nested",
    );
    check(mkdir(b"/vfs\0") == -17, b"mkdir exists");
    check(mkdir(b"/absent/sub\0") == -2, b"mkdir missing parent");
    check(mkdir(b"/hello.txt/sub\0") == -20, b"mkdir file parent");
    check(
        mkdir(b"/vfs/.\0") == -22 && mkdir(b"\0") == -2,
        b"mkdir invalid path",
    );
    check(
        syscall(MKDIRAT, [(-100isize) as usize, 0x3000, 0, 0, 0, 0]) == -14,
        b"mkdir bad pointer",
    );
    check(open(b"/\0", 1 << 30) == -22, b"root flags");
    write(1, b"V1 paths PASS\n");

    // M3：四参数 ABI、目录 fd 作为相对多段路径起点，以及目录句柄错误类型。
    let base = opened(b"/vfs\0", DIRECTORY);
    let nested = open_at(base as usize, b"sub/fromdir\0", 2 | CREATE | EXCL, 0o600);
    check(
        nested >= 0 && write(nested, b"Q") == 1,
        b"openat dirfd create",
    );
    close(nested);
    let nested = opened(b"/vfs/sub/fromdir\0", 0);
    let mut one = [0u8; 1];
    check(
        read(nested, &mut one) == 1 && one == *b"Q",
        b"openat dirfd content",
    );
    close(nested);
    let absolute = open_at(99, b"/hello.txt\0", 0, 0);
    check(absolute >= 0, b"absolute ignores dirfd");
    close(absolute);
    check(open_at(99, b"sub/no\0", 0, 0) == -9, b"relative bad dirfd");
    let regular = opened(b"/hello.txt\0", 0);
    check(
        open_at(regular as usize, b"child\0", 0, 0) == -20,
        b"relative file dirfd",
    );
    close(regular);
    check(
        open_at(0, b"name\0", 0, 0) == -20,
        b"relative console dirfd",
    );
    // mkdirat 与 openat 使用同一个目录 fd 起点；创建后可由绝对路径打开。
    check(
        mkdir_at(base as usize, b"sub/created\0") == 0,
        b"mkdirat dirfd create",
    );
    close(opened(b"/vfs/sub/created\0", DIRECTORY));
    check(
        mkdir_at(base as usize, b"sub/created\0") == -17,
        b"mkdirat dirfd exists",
    );
    check(
        mkdir_at(99, b"/vfs/sub/absolute\0") == 0,
        b"mkdirat absolute ignores dirfd",
    );
    close(opened(b"/vfs/sub/absolute\0", DIRECTORY));
    check(mkdir_at(99, b"missing\0") == -9, b"mkdirat bad dirfd");
    let regular = opened(b"/hello.txt\0", 0);
    check(
        mkdir_at(regular as usize, b"child\0") == -20,
        b"mkdirat file dirfd",
    );
    close(regular);
    check(mkdir_at(0, b"child\0") == -20, b"mkdirat console dirfd");
    check(
        syscall(OPENAT, [b"/hello.txt\0".as_ptr() as usize, 0, 0, 0, 0, 0]) == -14,
        b"old openat ABI rejected",
    );
    close(base);
    write(1, b"M3_OPENAT_PASS\n");
    write(1, b"M3_MKDIRAT_PASS\n");

    // 每次 open 独立偏移；dup 持有相同打开描述。
    let fd = opened(b"/vfs/data\0", 2 | CREATE | EXCL);
    check(write(fd, b"abcdef") == 6, b"write file");
    close(fd);
    check(
        open(b"/vfs/data\0", 2 | CREATE | EXCL) == -17,
        b"exclusive open",
    );
    let fd = opened(b"/vfs/data\0", 0);
    let independent = opened(b"/vfs/data\0", 0);
    let mut two = [0u8; 2];
    check(read(fd, &mut two) == 2 && two == *b"ab", b"first read");
    let shared = syscall(DUP, [fd as usize, 0, 0, 0, 0, 0]);
    check(
        shared >= 0 && read(shared, &mut two) == 2 && two == *b"cd",
        b"dup offset",
    );
    check(
        read(independent, &mut two) == 2 && two == *b"ab",
        b"independent offset",
    );
    check(seek(shared, -1, 1) == 3, b"seek current shared");
    check(
        read(fd, &mut two) == 2 && two == *b"de",
        b"seek dup shared offset",
    );
    check(seek(independent, 0, 2) == 6, b"seek end");
    check(read(independent, &mut two) == 0, b"seek end EOF");
    check(seek(independent, 100, 0) == 100, b"seek beyond EOF");
    check(read(independent, &mut two) == 0, b"seek beyond EOF read");
    check(seek(independent, 1, 0) == 1, b"seek set");
    check(
        read(independent, &mut two) == 2 && two == *b"bc",
        b"seek set content",
    );
    check(seek(fd, -1, 0) == -22, b"seek negative");
    check(seek(fd, 0, 1) == 5, b"failed seek keeps offset");
    check(seek(fd, 0, 9) == -22, b"seek whence");
    check(seek(fd, 0, 1) == 5, b"bad whence keeps offset");
    check(seek(fd, isize::MAX, 1) == -75, b"seek overflow");
    check(seek(fd, 0, 1) == 5, b"overflow keeps offset");
    check(seek(99, 0, 0) == -9, b"seek bad fd");
    check(seek(99, 0, 9) == -9, b"seek bad fd before whence");
    check(seek(0, 0, 0) == -29, b"seek console");
    close(shared);
    close(independent);
    // 持有旧描述时截断：旧偏移仍为 5，read 必须返回 EOF，不能切片越界。
    let trunc = opened(b"/vfs/data\0", 1 | TRUNC);
    close(trunc);
    check(read(fd, &mut two) == 0, b"truncate beyond EOF");
    close(fd);
    let first = opened(b"/vfs/append\0", 1 | CREATE | APPEND);
    let second = opened(b"/vfs/append\0", 1 | APPEND);
    check(
        write(first, b"A") == 1 && write(second, b"B") == 1 && write(first, b"C") == 1,
        b"append writes",
    );
    close(first);
    close(second);
    let fd = opened(b"/vfs/append\0", 0);
    let mut three = [0u8; 3];
    check(
        read(fd, &mut three) == 3 && three == *b"ABC",
        b"append content",
    );
    close(fd);
    write(1, b"V2 offsets truncate append PASS\n");
    write(1, b"M3_LSEEK_PASS\n");

    // 定位 IO 作用于同一文件内容，但绝不改变 dup 共享的读写游标。
    let positioned = opened(b"/vfs/positioned\0", 2 | CREATE | EXCL);
    check(write(positioned, b"abcdef") == 6, b"positioned seed");
    let alias = syscall(DUP, [positioned as usize, 0, 0, 0, 0, 0]);
    check(alias >= 0, b"positioned dup");
    check(pread(alias, &mut two, 2) == 2 && two == *b"cd", b"pread data");
    check(seek(positioned, 0, 1) == 6, b"pread keeps cursor");
    check(pwrite(alias, b"XY", 1) == 2, b"pwrite data");
    check(seek(positioned, 0, 1) == 6, b"pwrite keeps cursor");
    let mut six = [0u8; 6];
    check(
        pread(positioned, &mut six, 0) == 6 && six == *b"aXYdef",
        b"positioned content",
    );
    check(pread(positioned, &mut two, -1) == -22, b"pread negative");
    check(pwrite(positioned, b"!", -1) == -22, b"pwrite negative");
    check(
        syscall(PREAD64, [positioned as usize, 0x4000_0000, 1, 0, 0, 0]) == -14,
        b"pread bad pointer",
    );
    check(
        syscall(PWRITE64, [positioned as usize, 0x4000_0000, 1, 0, 0, 0]) == -14,
        b"pwrite bad pointer",
    );
    check(seek(positioned, 0, 1) == 6, b"positioned errors keep cursor");
    check(
        pread(positioned, &mut six, 0) == 6 && six == *b"aXYdef",
        b"pwrite error keeps content",
    );
    check(pread(99, &mut two, 0) == -9, b"pread bad fd");
    check(pwrite(99, b"x", 0) == -9, b"pwrite bad fd");
    check(pread(0, &mut two, 0) == -29, b"pread console");
    check(pwrite(1, b"x", 0) == -29, b"pwrite console");
    close(alias);
    close(positioned);
    let readonly = opened(b"/vfs/positioned\0", 0);
    check(pwrite(readonly, b"x", 0) == -13, b"pwrite readonly");
    close(readonly);
    let writeonly = opened(b"/vfs/positioned\0", 1);
    check(pread(writeonly, &mut two, 0) == -13, b"pread writeonly");
    close(writeonly);
    let dir = opened(b"/vfs/sub\0", DIRECTORY);
    check(pread(dir, &mut two, 0) == -21, b"pread dir");
    check(pwrite(dir, b"x", 0) == -21, b"pwrite dir");
    close(dir);
    let append = opened(b"/vfs/positioned\0", 1 | APPEND);
    check(pwrite(append, b"Z", 0) == 1, b"pwrite append");
    check(seek(append, 0, 1) == 0, b"pwrite append cursor");
    close(append);
    let readonly = opened(b"/vfs/positioned\0", 0);
    let mut seven = [0u8; 7];
    check(
        pread(readonly, &mut seven, 0) == 7 && seven == *b"aXYdefZ",
        b"pwrite append content",
    );
    close(readonly);
    write(1, b"M3_POSITIONED_IO_PASS\n");

    // stat 编码必须来自 inode 快照，不能误把打开游标当文件大小。
    let mut stat = StatSlot { bytes: [0xff; 128], canary: 0x12ab_34cd };
    let file = opened(b"/vfs/positioned\0", 0);
    check(
        syscall(FSTAT, [file as usize, stat.bytes.as_mut_ptr() as usize, 0, 0, 0, 0]) == 0,
        b"fstat file",
    );
    let file_ino = u64::from_ne_bytes(stat.bytes[8..16].try_into().unwrap());
    check(file_ino != 0, b"fstat inode");
    check(
        u32::from_ne_bytes(stat.bytes[16..20].try_into().unwrap()) & 0o170000 == 0o100000,
        b"fstat regular type",
    );
    check(
        i64::from_ne_bytes(stat.bytes[48..56].try_into().unwrap()) == 7
            && i32::from_ne_bytes(stat.bytes[56..60].try_into().unwrap()) == 4096
            && i64::from_ne_bytes(stat.bytes[64..72].try_into().unwrap()) == 1,
        b"fstat regular length",
    );
    check(stat.canary == 0x12ab_34cd, b"fstat 128-byte bound");
    check(
        syscall(FSTAT, [file as usize, 0x4000_0000, 0, 0, 0, 0]) == -14,
        b"fstat bad pointer",
    );
    close(file);
    let dir = opened(b"/vfs/sub\0", DIRECTORY);
    check(
        syscall(FSTAT, [dir as usize, stat.bytes.as_mut_ptr() as usize, 0, 0, 0, 0]) == 0,
        b"fstat directory",
    );
    check(
        u32::from_ne_bytes(stat.bytes[16..20].try_into().unwrap()) & 0o170000 == 0o040000
            && i64::from_ne_bytes(stat.bytes[48..56].try_into().unwrap()) == 0
            && u64::from_ne_bytes(stat.bytes[8..16].try_into().unwrap()) != file_ino,
        b"fstat directory fields",
    );
    close(dir);
    check(
        syscall(FSTAT, [0, stat.bytes.as_mut_ptr() as usize, 0, 0, 0, 0]) == 0,
        b"fstat console",
    );
    check(
        u32::from_ne_bytes(stat.bytes[16..20].try_into().unwrap()) & 0o170000 == 0o020000,
        b"fstat console type",
    );
    check(
        syscall(FSTAT, [99, stat.bytes.as_mut_ptr() as usize, 0, 0, 0, 0]) == -9,
        b"fstat bad fd",
    );
    check(stat.canary == 0x12ab_34cd, b"fstat canary final");
    write(1, b"M3_FSTAT_PASS\n");

    // 类型与失败路径。无效用户缓冲区不能先推进文件偏移。
    let fd = opened(b"hello.txt\0", 0);
    // 地址位于用户区间内但没有 PTE，必须由逐页检查拒绝。
    check(
        syscall(READ, [fd as usize, 0x4000_0000, 2, 0, 0, 0]) == -14,
        b"read bad pointer",
    );
    check(
        read(fd, &mut two) == 2 && two == *b"he",
        b"failed read offset",
    );
    check(
        syscall(
            GETDENTS,
            [fd as usize, two.as_mut_ptr() as usize, 2, 0, 0, 0],
        ) == -20,
        b"getdents file",
    );
    close(fd);
    check(
        open(b"/hello.txt\0", DIRECTORY) == -20 && open(b"/hello.txt/\0", 0) == -20,
        b"require directory",
    );
    check(open(b"/hello.txt/child\0", 0) == -20, b"open file parent");
    let dir = opened(b"//vfs//sub/\0", DIRECTORY);
    check(
        read(dir, &mut two) == -21 && write(dir, b"x") == -21,
        b"directory IO",
    );
    close(dir);
    write(1, b"V3 errors PASS\n");

    // 名字长度为 5 时必须为 NUL 预留第 25 字节，记录长度应为 32。
    check(mkdir(b"/vfs/list\0") == 0, b"mkdir list");
    let fd = opened(b"/vfs/list/abcde\0", CREATE | 2);
    close(fd);
    check(mkdir(b"/vfs/list/child\0") == 0, b"mkdir child");
    let dir = opened(b"/vfs/list\0", DIRECTORY);
    let mut small = [0u8; 24];
    check(
        syscall(
            GETDENTS,
            [dir as usize, small.as_mut_ptr() as usize, 24, 0, 0, 0],
        ) == -22,
        b"small dirent",
    );
    check(
        syscall(GETDENTS, [dir as usize, 0x4000_0000, 32, 0, 0, 0]) == -14,
        b"dirent bad pointer",
    );
    let ino = dirent(dir, b"abcde", 8, 1);
    check(seek(dir, 0, 0) == 0, b"seek dir rewind");
    check(dirent(dir, b"abcde", 8, 1) == ino, b"seek dir reread");
    check(seek(dir, 1, 2) == 3, b"seek dir end plus one");
    check(
        syscall(GETDENTS, [dir as usize, small.as_mut_ptr() as usize, 24, 0, 0, 0]) == 0,
        b"seek dir beyond EOF",
    );
    check(seek(dir, 1, 0) == 1, b"seek dir index");
    let shared = syscall(DUP, [dir as usize, 0, 0, 0, 0, 0]);
    check(shared >= 0, b"dup directory");
    check(dirent(shared, b"child", 4, 2) != ino, b"distinct inode");
    check(
        syscall(
            GETDENTS,
            [dir as usize, small.as_mut_ptr() as usize, 24, 0, 0, 0],
        ) == 0,
        b"directory EOF",
    );
    close(shared);
    close(dir);
    let dir = opened(b"/vfs/list\0", DIRECTORY);
    check(dirent(dir, b"abcde", 8, 1) == ino, b"stable inode");
    close(dir);
    write(1, b"V4 directory cursor and records PASS\n");

    // 父子在不同地址空间中共享打开描述。先等待子读完，再验证父偏移。
    let fd = opened(b"/fork.txt\0", 0);
    let pid = syscall(FORK, [0; 6]);
    check(pid >= 0, b"fork offsets");
    if pid == 0 {
        check(read(fd, &mut two) == 2 && two == *b"ab", b"child read");
        exit(7);
    }
    wait(pid, 7);
    check(
        read(fd, &mut two) == 2 && two == *b"cd",
        b"fork shared offset",
    );
    close(fd);
    write(1, b"V5 fork shared offset PASS\n");

    // 从真实文件复制 ELF 到用户创建的子目录，exec 必须经同一 VFS 树找到它。
    let source = opened(b"/vfs-exec\0", 0);
    let target = opened(b"/vfs/sub/tool\0", 1 | CREATE | EXCL);
    let mut chunk = [0u8; 512];
    loop {
        let n = read(source, &mut chunk);
        check(n >= 0, b"copy ELF read");
        if n == 0 {
            break;
        }
        check(write(target, &chunk[..n as usize]) == n, b"copy ELF write");
    }
    close(source);
    close(target);
    let pid = syscall(FORK, [0; 6]);
    check(pid >= 0, b"fork exec");
    if pid == 0 {
        // 失败 exec 必须保留当前进程，之后仍能成功 exec。
        check(
            syscall(EXEC, [b"/vfs/no\0".as_ptr() as usize, 0, 0, 0, 0, 0]) == -2,
            b"exec missing",
        );
        check(
            syscall(EXEC, [b"/vfs/sub\0".as_ptr() as usize, 0, 0, 0, 0, 0]) == -21,
            b"exec directory",
        );
        let path = b"/vfs/sub/tool\0";
        let argv = [path.as_ptr() as usize, b"nested\0".as_ptr() as usize, 0];
        syscall(
            EXEC,
            [path.as_ptr() as usize, argv.as_ptr() as usize, 0, 0, 0, 0],
        );
        fail(b"exec returned");
    }
    wait(pid, 7);
    write(1, b"V6 nested exec and argv PASS\nVFS_PASS\n");
    exit(0)
}
