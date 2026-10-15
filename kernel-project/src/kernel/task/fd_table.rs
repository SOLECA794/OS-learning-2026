//! 任务私有 FD 表：槽位属于进程，打开描述及偏移属于 VFS 的 Arc 对象。
//!
//! fork 复制槽位而共享描述；close/exit 释放引用，最后一个引用释放描述。
//! 控制台仍为明确的 FD 类型，VFS 不依赖任务调度和 UART 等待协议。

use crate::fs::{OpenFileDesc, Pipe};
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

/// 文件与目录共用一个打开描述类型，实际节点类型由 VFS 判定。
#[derive(Clone)]
pub enum FdEntry {
    /// stdin 及其 dup：read 从 UART RX 取字节；write 保持原控制台输出语义。
    ConsoleInput,
    /// stdout/stderr 及其 dup：read 保持旧的 EOF(0)；write 输出到控制台。
    ConsoleOutput,
    /// 打开文件：Arc 共享 → dup/fork 继承同一偏移。
    File(Arc<OpenFileDesc>),
    /// 管道读端：从环形缓冲区读。
    PipeRead(Arc<Pipe>),
    /// 管道写端：写入环形缓冲区。
    PipeWrite(Arc<Pipe>),
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
                Some(FdEntry::ConsoleInput),
                Some(FdEntry::ConsoleOutput),
                Some(FdEntry::ConsoleOutput),
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

    /// dup2：把 entry 放到指定 fd 位置；若该位置已有 fd 先 close。
    pub fn set(&mut self, fd: usize, entry: FdEntry) -> Result<(), isize> {
        // 扩展表到 fd+1 长。
        while self.entries.len() <= fd {
            self.entries.push(None);
        }
        self.entries[fd] = Some(entry);
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
