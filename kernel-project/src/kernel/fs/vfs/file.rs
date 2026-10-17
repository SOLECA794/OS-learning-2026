//! 打开文件描述：节点句柄、访问模式和共享偏移属于同一个对象。
//!
//! 每次 open 新建对象；dup/fork 只 clone Arc。锁顺序固定为描述偏移锁 →
//! inode 内容锁，不在锁内访问用户内存或调用调度器。

use super::node::NodeRef;
use super::{
    DirectoryEntry, FileMetadata, FileType, OpenOptions, SeekWhence, EACCES, EISDIR, ENOTDIR,
};
use crate::klib::sync::SpinLock;

pub(crate) struct OpenFileDesc {
    inode: NodeRef,
    offset: SpinLock<usize>,
    readable: bool,
    writable: bool,
    append: bool,
}

impl OpenFileDesc {
    pub(super) fn new(inode: NodeRef, options: OpenOptions) -> Self {
        Self {
            inode,
            offset: SpinLock::new(0),
            readable: options.read,
            writable: options.write,
            append: options.append,
        }
    }

    pub(crate) fn is_dir(&self) -> bool {
        self.inode.kind() == FileType::Directory
    }

    /// 元数据来自 inode 本身，不依赖或改变共享打开偏移。
    pub(crate) fn metadata(&self) -> FileMetadata {
        self.inode.metadata()
    }

    /// 对共享打开描述原子地计算并提交偏移；错误不会改变原偏移。
    pub(crate) fn seek(&self, delta: i64, whence: SeekWhence) -> Result<usize, isize> {
        const EOVERFLOW: isize = -75;
        const EINVAL: isize = -22;
        let mut offset = self.offset.lock();
        let base = match whence {
            SeekWhence::Start => 0,
            SeekWhence::Current => *offset,
            SeekWhence::End => self.inode.seek_len(),
        };
        let target = base as i128 + delta as i128;
        if target < 0 {
            return Err(EINVAL);
        }
        if target > isize::MAX as i128 {
            return Err(EOVERFLOW);
        }
        *offset = target as usize;
        Ok(*offset)
    }

    /// VFS 内部解析相对路径的起点；不向 syscall 暴露 inode 或内部锁。
    pub(super) fn directory_inode(&self) -> Result<NodeRef, isize> {
        if !self.is_dir() {
            return Err(ENOTDIR);
        }
        Ok(self.inode.clone())
    }

    pub(crate) fn read(&self, buf: &mut [u8]) -> Result<usize, isize> {
        if self.is_dir() {
            return Err(EISDIR);
        }
        if !self.readable {
            return Err(EACCES);
        }
        let mut offset = self.offset.lock();
        let n = self.inode.read_at(*offset, buf)?;
        *offset += n;
        Ok(n)
    }

    /// 定位读不触碰共享游标；访问模式与普通 read 相同。
    pub(crate) fn read_at(&self, at: usize, buf: &mut [u8]) -> Result<usize, isize> {
        if self.is_dir() {
            return Err(EISDIR);
        }
        if !self.readable {
            return Err(EACCES);
        }
        self.inode.read_at(at, buf)
    }

    pub(crate) fn write(&self, bytes: &[u8]) -> Result<usize, isize> {
        if self.is_dir() {
            return Err(EISDIR);
        }
        if !self.writable {
            return Err(EACCES);
        }
        let mut offset = self.offset.lock();
        *offset = self.inode.write_at(*offset, bytes, self.append)?;
        Ok(bytes.len())
    }

    /// 定位写不触碰共享游标；O_APPEND 仍由 inode 内容锁选择文件末尾。
    pub(crate) fn write_at(&self, at: usize, bytes: &[u8]) -> Result<usize, isize> {
        if self.is_dir() {
            return Err(EISDIR);
        }
        if !self.writable {
            return Err(EACCES);
        }
        self.inode.write_at(at, bytes, self.append)?;
        Ok(bytes.len())
    }

    /// 调用方接受一条元数据后才推进游标。false 表示缓冲已满，Err 保持当前项。
    /// emit 只处理内核缓冲，不得阻塞、调度或反向访问同一个打开描述。
    pub(crate) fn read_dir(
        &self,
        mut emit: impl FnMut(&DirectoryEntry, usize) -> Result<bool, isize>,
    ) -> Result<(), isize> {
        let mut cursor = self.offset.lock();
        while let Some(entry) = self.inode.dir_entry(*cursor)? {
            if !emit(&entry, *cursor + 1)? {
                break;
            }
            *cursor += 1;
        }
        Ok(())
    }
}
