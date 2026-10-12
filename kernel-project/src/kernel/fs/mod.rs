//! 文件系统子系统（V2 §9：ramfs、FD、打开文件对象；程序源先于 exec 可用）。
//!
//! - `programs`：命名只读程序源（G2/G4 先于文件系统落地，G5 起作为 ramfs 装载源）。
//! - `ramfs`：内容唯一权威；open/read/write/close 的底层文件操作。
//! - `file`：OpenFileDesc（共享偏移单元）与进程 FdTable。
//! - syscall 层经 `task::with_current_fds` 显式取得当前任务 FD 表（V2 §3，不读 Task 私有字段）。

pub(crate) mod file;
pub(crate) mod programs;
pub(crate) mod ramfs;

/// boot 装载 ramfs（在用户任务创建前调用）。
pub fn init() {
    ramfs::init();
}
