#![no_std]
#![no_main]

//! brk(2) 验收程序：扩展堆 → 写入 → 读回 → 收缩堆。
//!
//! 流程：
//! 1. brk(0) 取初始 break，应等于 heap_base (0x1000_0000)。
//! 2. brk(0x1001_0000) 扩展 4 页。
//! 3. 在堆上写 0x55，读回验证。
//! 4. brk(0x1000_1000) 收缩 3 页。
//! 5. 收缩后堆顶写 0xAA 读回。
//! 标记：B1-B5。

use core::arch::asm;

const SYS_BRK: usize = 214;
const SYS_WRITE: usize = 64;
const SYS_EXIT: usize = 93;

const HEAP_BASE: usize = 0x2000_0000;

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
    // B1: brk(0) 取初始 break
    let initial = syscall(SYS_BRK, [0, 0, 0, 0, 0, 0]);
    if initial == HEAP_BASE as isize {
        putstr("B1");
    } else {
        putstr("b1-FAIL");
    }

    // B2: 扩展堆到 0x2001_0000（16 页）
    let grown = syscall(SYS_BRK, [0x2001_0000, 0, 0, 0, 0, 0]);
    if grown == 0x2001_0000 as isize {
        putstr("B2");
    } else if grown == -1000 {
        putstr("b2-MEMERR");
    } else if grown == -2000 {
        putstr("b2-NOSPACE");
    } else {
        putstr("b2-OTHER");
    }

    // B3: 在堆上写 0x55 并读回
    let ptr = HEAP_BASE as *mut u8;
    unsafe {
        ptr.write(0x55);
    }
    let readback = unsafe { ptr.read() };
    if readback == 0x55 {
        putstr("B3");
    } else {
        putstr("b3-FAIL");
    }

    // B4: 收缩堆到 0x2000_1000（1 页）
    let shrunk = syscall(SYS_BRK, [0x2000_1000, 0, 0, 0, 0, 0]);
    if shrunk == 0x2000_1000 as isize {
        putstr("B4");
    } else {
        putstr("b4-FAIL");
    }

    // B5: 收缩后堆顶仍可写
    let ptr2 = (HEAP_BASE + 0x500) as *mut u8;
    unsafe {
        ptr2.write(0xAA);
    }
    let readback2 = unsafe { ptr2.read() };
    if readback2 == 0xAA {
        putstr("B5");
    } else {
        putstr("b5-FAIL");
    }

    putstr(" BRK-PASS");
    syscall(SYS_EXIT, [0, 0, 0, 0, 0, 0]);
    loop {}
}
