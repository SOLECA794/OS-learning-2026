#![no_std]
#![no_main]

//! 由 VFS 用例复制进子目录后执行，检查 argv 确实来自新用户栈。

use core::arch::asm;

fn syscall(id: usize, a0: usize, a1: usize, a2: usize) -> isize {
    let ret: isize;
    unsafe {
        asm!("ecall", inlateout("a0") a0 => ret, in("a1") a1, in("a2") a2,
            in("a7") id, options(nostack));
    }
    ret
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    exit(1)
}

fn exit(code: usize) -> ! {
    syscall(93, code, 0, 0);
    loop {}
}

#[no_mangle]
extern "C" fn _start(argc: usize, argv: usize) -> ! {
    if argc != 2 || argv == 0 {
        exit(1);
    }
    // SAFETY: 本用例入口的 argv 由 exec 构造；只读取已声明的两个参数及终止项。
    let args = unsafe { core::slice::from_raw_parts(argv as *const usize, 3) };
    if args[0] == 0 || args[1] == 0 || args[2] != 0 {
        exit(2);
    }
    // 两个期望字符串均包含 NUL；比较也覆盖字符串终止。
    let path = b"/vfs/sub/tool\0";
    let arg = b"nested\0";
    let correct = unsafe {
        core::slice::from_raw_parts(args[0] as *const u8, path.len()) == path
            && core::slice::from_raw_parts(args[1] as *const u8, arg.len()) == arg
    };
    if !correct {
        exit(3);
    }
    let message = b"VFS_EXEC_ARGS_PASS\n";
    syscall(64, 1, message.as_ptr() as usize, message.len());
    exit(7)
}
