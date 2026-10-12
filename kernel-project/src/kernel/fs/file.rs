//! 打开文件对象与进程 FD 表（V2 §9：open/dup/close 合同）。
//!
//! 模型：
//! - `OpenFileDesc`（打开文件描述）是 **dup/fork 共享的单元**：持有文件名的
//!   内容引用（内容唯一权威在 ramfs）与打开偏移（`AtomicUsize`，共享对象上的
//!   唯一权威来源——不在 Task、FD 表、文件系统三处各存一份）。
//! - `FdEntry` 用具体 enum 表达（V2 §9：第二个文件后端出现前不抽象 VFS）：
//!   控制台或打开文件。
//! - `FdTable` 是任务私有 FD 表；fork 时整表复制（`Arc<OpenFileDesc>` 共享同一
//!   偏移），close/进程退出把槽位置 None（Arc 计数归零触发 desc 释放）。
//! - 文件内容读写一律经 `crate::kernel::fs::ramfs` 的全局锁，本模块不持内容副本。

use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::AtomicUsize;

/// 打开文件描述：共享对象（dup/fork 共享同一实例 → 共享同一偏移）。
pub struct OpenFileDesc {
    /// ramfs 文件名（不含前导 '/'）；内容唯一权威在 ramfs。
    pub name: Vec<u8>,
    /// 打开偏移（唯一权威来源，Atomic 支持 Arc 共享下更新）。
    pub offset: AtomicUsize,
    /// open 时声明的读写权限（O_RDONLY/O_WRONLY/O_RDWR）。
    pub readable: bool,
    pub writable: bool,
}

impl OpenFileDesc {
    pub fn new(name: Vec<u8>, readable: bool, writable: bool) -> Self {
        Self {
            name,
            offset: AtomicUsize::new(0),
            readable,
            writable,
        }
    }
}

/// FD 表项：控制台、打开文件或目录（V2 §9 具体 enum，不抽象 VFS）。
#[derive(Clone)]
pub enum FdEntry {
    /// 标准 I/O（fd 0/1/2）：read 返回 EOF(0)，write 输出到控制台。
    Console,
    /// 打开文件：Arc 共享 → dup/fork 继承同一偏移。
    File(Arc<OpenFileDesc>),
    /// 打开的目录（G5 shell：ramfs 根目录）：枚举游标（getdents64 进度）。
    Dir(Arc<AtomicUsize>),
}

/// 进程 FD 表：fd = Vec 索引；None = 空槽（可复用）。
pub struct FdTable {
    entries: Vec<Option<FdEntry>>,
}

impl FdTable {
    /// 默认表：fd 0/1/2 均为控制台。
    pub fn new() -> Self {
        Self {
            entries: vec![
                Some(FdEntry::Console),
                Some(FdEntry::Console),
                Some(FdEntry::Console),
            ],
        }
    }

    /// 分配新 fd：复用最小空槽，无空槽则追加。
    pub fn alloc(&mut self, entry: FdEntry) -> Result<usize, isize> {
        for (fd, slot) in self.entries.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some(entry);
                return Ok(fd);
            }
        }
        let fd = self.entries.len();
        self.entries.push(Some(entry));
        Ok(fd)
    }

    /// 取指定 fd 的表项引用；非法 fd 返回 None（调用方映射 EBADF）。
    pub fn get(&self, fd: usize) -> Option<&FdEntry> {
        self.entries.get(fd).and_then(|s| s.as_ref())
    }

    /// 关闭 fd：槽位置 None；非法 fd 返回 EBADF。
    pub fn close(&mut self, fd: usize) -> Result<(), isize> {
        let slot = self.entries.get_mut(fd).ok_or(EBADF)?;
        if slot.is_none() {
            return Err(EBADF);
        }
        *slot = None;
        Ok(())
    }
}

/// 克隆（fork 用）：复制 FD 表；File 项 Arc clone → 父子共享同一打开文件描述。
impl Clone for FdTable {
    fn clone(&self) -> Self {
        Self {
            entries: self.entries.clone(),
        }
    }
}

const EBADF: isize = -9;
