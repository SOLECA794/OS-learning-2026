//! 文件系统子系统（G4 最小：命名程序源；G5 扩展 ramfs/FD）。
//!
//! V2 §3：fs 通过显式参数取得调用上下文，不读取 Task 私有字段；
//! 程序源是 exec 的确定性交接点。

#[cfg(feature = "arch-riscv64")]
pub(crate) mod programs;
