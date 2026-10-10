#![no_std]
#![no_main]

mod klib;

mod arch;
mod kernel;

use core::panic::PanicInfo;

/// M0 的 panic 处理：打印位置后停机。
///
/// 刻意不做恢复、不做重启——内核 panic 后继续运行会把损坏的状态扩散到更多子系统。
/// 退出条件：M1 引入内核自检后，这里改为打印栈回溯（需要 frame pointer 支持）。
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    match info.location() {
        Some(loc) => {
            crate::log_error!("panic at {}:{}: {}", loc.file(), loc.line(), info.message())
        }
        None => crate::log_error!("panic: {}", info.message()),
    }
    crate::arch::park()
}
