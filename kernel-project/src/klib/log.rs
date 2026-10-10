//! 日志宏与级别过滤（docs/01 §5 把日志宏归到 klib）。
//!
//! ## 依赖倒置
//! klib 位于依赖树最底层，不能反向依赖 `arch`。字节输出能力因此由 arch 以链接期
//! 符号 `kernel_arch_console_write` 提供（见 `src/arch/<arch>/console.rs`）。
//! 若某个架构后端没有实现该符号，**链接期就会失败**——不允许"日志被静默丢弃"。
//!
//! ## 级别裁剪
//! 级别由 Cargo feature `log-level-*` 决定（Kconfig 的 `choice 日志级别` → build.mk →
//! Cargo feature）。低于阈值的日志在编译期就被常量折叠掉，不占产物体积。

extern "C" {
    fn kernel_arch_console_write(ptr: *const u8, len: usize);
}

pub const ERROR: u8 = 1;
pub const WARN: u8 = 2;
pub const INFO: u8 = 3;
pub const DEBUG: u8 = 4;
pub const TRACE: u8 = 5;

/// 当前编译进内核的最低日志级别。
///
/// 未指定任何 `log-level-*` feature 时回落到 INFO：日志不该因为漏配而全静默，
/// 但正常构建路径（build.mk）总会带上一个级别，回落只在手工 `cargo` 调用时出现。
pub const LEVEL: u8 = {
    if cfg!(feature = "log-level-trace") {
        TRACE
    } else if cfg!(feature = "log-level-debug") {
        DEBUG
    } else if cfg!(feature = "log-level-info") {
        INFO
    } else if cfg!(feature = "log-level-warn") {
        WARN
    } else if cfg!(feature = "log-level-error") {
        ERROR
    } else {
        INFO
    }
};

/// 单条日志的栈上缓冲上限。
///
/// 已知限制：单行日志超过 `LINE_LIMIT` 字节会被截断（内核早期没有堆，不能动态扩容）。
/// 退出条件：M1 的 kalloc 落地后，超长日志改为分多次 `kernel_arch_console_write` 输出。
const LINE_LIMIT: usize = 512;

fn write_bytes(bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }
    unsafe { kernel_arch_console_write(bytes.as_ptr(), bytes.len()) }
}

/// 打印一条格式化日志。由 `log_*!` 宏调用，不直接暴露给调用者。
pub fn print(args: core::fmt::Arguments) {
    // 常见情形是纯字面量，直接输出可避免缓冲与截断。
    if let Some(s) = args.as_str() {
        write_bytes(s.as_bytes());
        return;
    }
    let mut buf = LineBuffer {
        bytes: [0; LINE_LIMIT],
        len: 0,
    };
    let _ = core::fmt::write(&mut buf, args);
    write_bytes(&buf.bytes[..buf.len]);
}

struct LineBuffer {
    bytes: [u8; LINE_LIMIT],
    len: usize,
}

impl core::fmt::Write for LineBuffer {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let room = LINE_LIMIT - self.len;
        let n = s.len().min(room);
        self.bytes[self.len..self.len + n].copy_from_slice(&s.as_bytes()[..n]);
        self.len += n;
        Ok(())
    }
}

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        if $crate::klib::log::ERROR <= $crate::klib::log::LEVEL {
            $crate::klib::log::print(format_args!($($arg)*))
        }
    };
}

#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {
        if $crate::klib::log::WARN <= $crate::klib::log::LEVEL {
            $crate::klib::log::print(format_args!($($arg)*))
        }
    };
}

#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        if $crate::klib::log::INFO <= $crate::klib::log::LEVEL {
            $crate::klib::log::print(format_args!($($arg)*))
        }
    };
}

#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => {
        if $crate::klib::log::DEBUG <= $crate::klib::log::LEVEL {
            $crate::klib::log::print(format_args!($($arg)*))
        }
    };
}

#[macro_export]
macro_rules! log_trace {
    ($($arg:tt)*) => {
        if $crate::klib::log::TRACE <= $crate::klib::log::LEVEL {
            $crate::klib::log::print(format_args!($($arg)*))
        }
    };
}
