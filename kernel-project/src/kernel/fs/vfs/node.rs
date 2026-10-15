//! 后端节点的最小对象安全接口；VFS 不持有具体文件系统的节点类型。

use alloc::sync::Arc;
use alloc::vec::Vec;

use super::{DirectoryEntry, FileMetadata, FileType};

pub(in crate::kernel::fs) type NodeRef = Arc<dyn VfsNode>;

/// `child` 必须在后端的目录锁内完成查找/创建，保持 O_EXCL 原子语义。
pub(in crate::kernel::fs) trait VfsNode: Send + Sync {
    fn metadata(&self) -> FileMetadata;
    fn seek_len(&self) -> usize;
    fn child(
        &self,
        name: &[u8],
        create: Option<FileType>,
        exclusive: bool,
    ) -> Result<NodeRef, isize>;
    fn truncate(&self) -> Result<(), isize>;
    fn read_at(&self, offset: usize, buf: &mut [u8]) -> Result<usize, isize>;
    fn write_at(&self, offset: usize, bytes: &[u8], append: bool) -> Result<usize, isize>;
    fn read_all(&self) -> Result<Vec<u8>, isize>;
    fn dir_entry(&self, index: usize) -> Result<Option<DirectoryEntry>, isize>;

    fn kind(&self) -> FileType {
        self.metadata().file_type
    }
}
