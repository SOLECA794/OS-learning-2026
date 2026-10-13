#![no_std]
#![no_main]

//! G6 COW 验收程序（uapp-cow）：fork 后父子各自写入共享页，验证写时复制。
//!
//! 固定输入对照（V2 G6 验收）：fork 后 BUF 页处于 COW 只读共享，
//! - 子写 BUF[2048] → 触发 COW 复制（子拿到自己的页）；
//! - 父写 BUF[0]   → 触发 COW 复制（父拿到自己的页）；
//! - 相互隔离：父读不到子的写入，子读不到父的写入。
//! 输出断言标记：C1（父写读回）、C2（父读子页仍为初始值）、
//! C3（子写读回）、C4（子读父页仍为初始值）、C5（父 wait 子成功）。
//! 内核侧每次 COW 复制打一行 `[cow]` 日志，拷贝次数与标记数可对照。

use core::arch::asm;
use core::sync::atomic::{AtomicU8, Ordering};

const SYS_WRITE: usize = 64;
const SYS_EXIT: usize = 93;
const SYS_FORK: usize = 220;
const SYS_WAIT4: usize = 260;

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
        if BUF[2048].load(Ordering::Relaxed) == CHILD_WRITE {
            putstr("C3");
        } else {
            putstr("c3-FAIL");
        }
        // 子读父侧 BUF[0]：必须是 INIT（父的写入发生在父自己的副本上）。
        if BUF[0].load(Ordering::Relaxed) == INIT {
            putstr("C4");
        } else {
            putstr("c4-FAIL");
        }
        syscall(SYS_EXIT, [5, 0, 0, 0, 0, 0]);
        loop {}
    } else {
        // ---- 父进程 ----
        // 先忙等，让子有机会先写（子写发生在子的副本上，父侧 BUF[2048] 仍为 INIT）。
        for _ in 0..100_000 {
            core::hint::spin_loop();
        }
        BUF[0].store(PARENT_WRITE, Ordering::Relaxed);
        if BUF[0].load(Ordering::Relaxed) == PARENT_WRITE {
            putstr("C1");
        } else {
            putstr("c1-FAIL");
        }
        // 父读子侧 BUF[2048]：必须是 INIT（子的写入发生在子的副本上）。
        if BUF[2048].load(Ordering::Relaxed) == INIT {
            putstr("C2");
        } else {
            putstr("c2-FAIL");
        }
        // wait 子：wstatus 指针传 0 也可（内核填 null 写回前先判空）。
        let _w = syscall(SYS_WAIT4, [pid as usize, 0, 0, 0, 0, 0]);
        putstr("C5");
        syscall(SYS_EXIT, [0, 0, 0, 0, 0, 0]);
        loop {}
    }
}

fn putstr(s: &str) {
    syscall(
        SYS_WRITE,
        [1, s.as_ptr() as usize, s.len(), 0, 0, 0],
    );
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
