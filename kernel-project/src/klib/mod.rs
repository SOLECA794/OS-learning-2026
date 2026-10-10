//! klib：依赖树最底层的通用内核库（docs/01 §5）。
//!
//! 分层约束：klib 不允许反向依赖 arch / kernel / fs / driver / net。
//! 需要架构能力时走依赖倒置（见 log 模块的 `kernel_arch_console_write`）。

// log_*! 宏用 #[macro_export] 导出到 crate 根，调用处写 crate::log_info!(...)。
// 不用 #[macro_use]：那样会依赖模块声明顺序，且未调用的级别宏会触发 unused_macros 警告。
pub mod log;
