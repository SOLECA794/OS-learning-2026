#![no_std]
#![no_main]

//! G6 COW 验收程序（uapp-cow）：fork 后父子各自写入共享页，验证写时复制。
//!
//! 固定输入对照（V2 G6 验收）：fork 后 BUF 页处于 COW 只读共享，
//! - 子写 BUF[2048] → 触发 COW 复制（子拿到自己的页）；
//! - 父写 BUF[0]   → 触发 COW 复制（父拿到自己的页）；
//! - 相互隔离：父读不到子的写入，子读不到父的写入。
//! 输出断言标记：C1（父写读回）、C2（父读子页仍为初始值）、
//! C3（子写读回）、C4（子读父页仍为初始值）、C5（父 wait 子成功）、
//! C6（内核向 COW 状态页写回正确的退出码）、C7（read 写入另一 COW 页）、
//! C8（未映射但处于用户地址范围内的指针返回 EFAULT，内核继续运行）。
//! 额外验收：wait4 写回失败可重试、重复回收、子进程再次 fork 的多代共享。
//! 内核侧每次 COW 复制打一行 `[cow]` 日志，拷贝次数与标记数可对照。

use core::arch::asm;
use core::sync::atomic::{AtomicU8, Ordering};

const SYS_WRITE: usize = 64;
const SYS_EXIT: usize = 93;
const SYS_FORK: usize = 220;
const SYS_WAIT4: usize = 260;
const SYS_OPENAT: usize = 56;
const AT_FDCWD: usize = (-100isize) as usize;
const SYS_READ: usize = 63;
const SYS_CLOSE: usize = 57;

/// 32 位 Linux wait 状态与紧邻 canary：旧的 8 字节写回会破坏 canary。
#[repr(C)]
struct WaitStatusSlot {
    status: i32,
    canary: u32,
}
static mut WAIT_STATUS: WaitStatusSlot = WaitStatusSlot {
    status: -1,
    canary: 0xa55a_5aa5,
};

/// 独立页保证 read 目的地不会被 BUF/WAIT_STATUS 的先前写入提前解除 COW。
#[repr(align(4096))]
struct AlignedReadBuf([u8; 4096]);
static mut READ_BUF: AlignedReadBuf = AlignedReadBuf([0xAA; 4096]);

/// 4 KiB 共享缓冲区（跨一页），初始值 0xAA。
static BUF: [AtomicU8; 4096] = [const { AtomicU8::new(0xAA) }; 4096];

const INIT: u8 = 0xAA;
const PARENT_WRITE: u8 = 0x11;
const CHILD_WRITE: u8 = 0x22;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[no_mangle]
extern "C" fn _start(_argc: usize, _argv: usize) -> ! {
    let pid = syscall(SYS_FORK, [0, 0, 0, 0, 0, 0]);
    if pid == 0 {
        // ---- 子进程 ----
        // 先忙等，让父有机会先写（若父已写，子的共享页仍为 INIT，正好验证隔离）。
        for _ in 0..100_000 {
            core::hint::spin_loop();
        }
        BUF[2048].store(CHILD_WRITE, Ordering::Relaxed);
        let child_write_ok = BUF[2048].load(Ordering::Relaxed) == CHILD_WRITE;
        if child_write_ok {
            putstr("C3");
        } else {
            putstr("c3-FAIL");
        }
        // 子读父侧 BUF[0]：必须是 INIT（父的写入发生在父自己的副本上）。
        let child_isolated = BUF[0].load(Ordering::Relaxed) == INIT;
        if child_isolated {
            putstr("C4");
        } else {
            putstr("c4-FAIL");
        }
        // 父进程通过退出码汇总子侧断言，不能只打印失败却仍报告成功。
        syscall(
            SYS_EXIT,
            [
                if child_write_ok && child_isolated {
                    5
                } else {
                    1
                },
                0,
                0,
                0,
                0,
                0,
            ],
        );
        loop {}
    } else {
        // ---- 父进程 ----
        // 先忙等，让子有机会先写（子写发生在子的副本上，父侧 BUF[2048] 仍为 INIT）。
        for _ in 0..100_000 {
            core::hint::spin_loop();
        }
        BUF[0].store(PARENT_WRITE, Ordering::Relaxed);
        let parent_write_ok = BUF[0].load(Ordering::Relaxed) == PARENT_WRITE;
        if parent_write_ok {
            putstr("C1");
        } else {
            putstr("c1-FAIL");
        }
        // 父读子侧 BUF[2048]：必须是 INIT（子的写入发生在子的副本上）。
        let parent_isolated = BUF[2048].load(Ordering::Relaxed) == INIT;
        if parent_isolated {
            putstr("C2");
        } else {
            putstr("c2-FAIL");
        }
        // read 直接写入独立的 COW 页，再核对文件内容。
        let path = b"/hello.txt\0";
        let fd = syscall(SYS_OPENAT, [AT_FDCWD, path.as_ptr() as usize, 0, 0, 0, 0]);
        // SAFETY: 只计算静态缓冲区首元素的原始地址，不创建可变引用。
        let read_ptr =
            unsafe { core::ptr::addr_of_mut!((*core::ptr::addr_of_mut!(READ_BUF)).0) as *mut u8 };
        let read_len = if fd >= 0 {
            let n = syscall(SYS_READ, [fd as usize, read_ptr as usize, 11, 0, 0, 0]);
            syscall(SYS_CLOSE, [fd as usize, 0, 0, 0, 0, 0]);
            n
        } else {
            -1
        };
        // SAFETY: read 已返回；这里只读本进程私有的用户缓冲区。
        let read_ok = read_len == 11
            && unsafe { core::slice::from_raw_parts(read_ptr, 11) == b"hello ramfs" };
        if fd < 0 {
            putstr("C7-open-fail");
        } else if read_len != 11 {
            putstr("C7-read-fail");
        } else if !read_ok {
            putstr("C7-data-fail");
        }
        // 非空 wstatus：内核写入仍共享的用户页，必须先完成 COW。
        // SAFETY: 只构造静态字段的原始指针；父进程此时独占其地址空间。
        let status_ptr = unsafe { core::ptr::addr_of_mut!(WAIT_STATUS.status) };
        // 失败写回不得消费僵尸：同一个子进程可以重复等待，最后成功回收。
        let bad_wait = syscall(SYS_WAIT4, [pid as usize, 0x4000_0000, 0, 0, 0, 0]);
        let retry_wait = syscall(SYS_WAIT4, [pid as usize, 0x4000_0000, 0, 0, 0, 0]);
        let waited = syscall(SYS_WAIT4, [pid as usize, status_ptr as usize, 0, 0, 0, 0]);
        if waited == pid {
            putstr("C5");
        } else {
            putstr("c5-FAIL");
        }
        // SAFETY: 子进程已被回收；本进程是 WAIT_STATUS 的唯一写入者。
        let status = unsafe { core::ptr::read_volatile(status_ptr) };
        // SAFETY: wait 已返回，读取同一地址空间内的已初始化 canary。
        let canary = unsafe { core::ptr::read_volatile(core::ptr::addr_of!(WAIT_STATUS.canary)) };
        let width_ok = canary == 0xa55a_5aa5;
        if width_ok {
            putstr(" M3-WAIT32-PASS ");
        } else {
            putstr(" M3-WAIT32-FAIL ");
        }
        if waited == pid && status == (5 << 8) {
            putstr("C6");
        } else {
            putstr("c6-FAIL");
        }
        if waited == pid && status == (5 << 8) && read_ok {
            putstr("C7");
        } else {
            putstr("c7-FAIL");
        }
        // 0x4000_0000 位于许可的用户虚拟地址范围内，但不属于 ELF/用户栈映射。
        let invalid_write = syscall(SYS_WRITE, [1, 0x4000_0000, 1, 0, 0, 0]);
        if invalid_write != -14 {
            putstr("C8-errno-fail");
        }
        let reaped_once = syscall(SYS_WAIT4, [pid as usize, 0, 0, 0, 0, 0]) == -10;
        let retry_ok = bad_wait == -14 && retry_wait == -14 && reaped_once;
        if !retry_ok {
            putstr(" WAIT-RETRY-FAIL ");
        }
        // 重复 fork/exit/wait，覆盖 COW 页与内核栈的反复释放（任务 ID 暂不复用）。
        let mut repeat_ok = true;
        for _ in 0..128 {
            let child = syscall(SYS_FORK, [0; 6]);
            if child == 0 {
                syscall(SYS_EXIT, [0; 6]);
                loop {
                    core::hint::spin_loop();
                }
            }
            if child < 0 || syscall(SYS_WAIT4, [child as usize, 0, 0, 0, 0, 0]) != child {
                repeat_ok = false;
                break;
            }
            if syscall(SYS_WAIT4, [child as usize, 0, 0, 0, 0, 0]) != -10 {
                repeat_ok = false;
            }
        }
        // 多代 fork：子空间必须拥有完整映射区描述，才能继续派生孙进程并正确释放。
        let child = syscall(SYS_FORK, [0; 6]);
        if child == 0 {
            let grandchild = syscall(SYS_FORK, [0; 6]);
            if grandchild == 0 {
                BUF[0].store(0x33, Ordering::Relaxed);
                syscall(SYS_EXIT, [7, 0, 0, 0, 0, 0]);
                loop {
                    core::hint::spin_loop();
                }
            }
            let mut status: i32 = -1;
            let waited = syscall(
                SYS_WAIT4,
                [
                    grandchild as usize,
                    core::ptr::addr_of_mut!(status) as usize,
                    0,
                    0,
                    0,
                    0,
                ],
            );
            let ok = grandchild > 0
                && waited == grandchild
                && status == (7 << 8)
                && BUF[0].load(Ordering::Relaxed) == PARENT_WRITE;
            syscall(SYS_EXIT, [if ok { 9 } else { 1 }, 0, 0, 0, 0, 0]);
            loop {
                core::hint::spin_loop();
            }
        }
        let mut nested_status: i32 = -1;
        let nested_wait = syscall(
            SYS_WAIT4,
            [
                child as usize,
                core::ptr::addr_of_mut!(nested_status) as usize,
                0,
                0,
                0,
                0,
            ],
        );
        let nested_ok = child > 0
            && nested_wait == child
            && nested_status == (9 << 8)
            && BUF[0].load(Ordering::Relaxed) == PARENT_WRITE;
        if pid > 0
            && parent_write_ok
            && parent_isolated
            && waited == pid
            && status == (5 << 8)
            && read_ok
            && invalid_write == -14
            && retry_ok
            && repeat_ok
            && nested_ok
            && width_ok
        {
            putstr("C8");
            putstr(" WAIT-RETRY-PASS ");
            // 唯一成功标记：汇总父子隔离、错误重试和重复回收断言。
            putstr(" G6-COW-PASS");
        } else {
            putstr("c8-FAIL");
        }
        syscall(SYS_EXIT, [0, 0, 0, 0, 0, 0]);
        loop {}
    }
}

fn putstr(s: &str) {
    syscall(SYS_WRITE, [1, s.as_ptr() as usize, s.len(), 0, 0, 0]);
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
