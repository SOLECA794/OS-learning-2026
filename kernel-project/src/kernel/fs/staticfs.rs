//! 固定只读文件树：用于验证 VFS 能把同一路径/FD 语义交给第二个后端。
//!
//! 不是 ext4 或持久化文件系统；由 VFS 固定挂载在 `/static`。

use alloc::sync::Arc;
use alloc::vec::Vec;

use super::vfs::node::{NodeRef, VfsNode};
use super::vfs::{DirectoryEntry, FileMetadata, FileType, EEXIST, EISDIR, ENOENT, ENOTDIR};

const EROFS: isize = -30;
const CONTENT: &[u8] = b"staticfs via vfs\n";
const FILE_INO: usize = usize::MAX;

enum Node {
    Root(usize),
    Readme,
}

/// 每个节点无可变状态；打开偏移只存在于 VFS 的打开描述中。
pub(super) fn root(inode_number: usize) -> NodeRef {
    Arc::new(Node::Root(inode_number))
}

impl VfsNode for Node {
    fn metadata(&self) -> FileMetadata {
        match self {
            Node::Root(inode_number) => FileMetadata {
                inode_number: *inode_number,
                file_type: FileType::Directory,
                size: 0,
            },
            Node::Readme => FileMetadata {
                inode_number: FILE_INO,
                file_type: FileType::Regular,
                size: CONTENT.len(),
            },
        }
    }

    fn seek_len(&self) -> usize {
        match self {
            Node::Root(_) => 1,
            Node::Readme => CONTENT.len(),
        }
    }

    fn child(
        &self,
        name: &[u8],
        create: Option<FileType>,
        exclusive: bool,
    ) -> Result<NodeRef, isize> {
        if !matches!(self, Node::Root(_)) {
            return Err(ENOTDIR);
        }
        if name == b"readme" {
            if create.is_some() && exclusive {
                return Err(EEXIST);
            }
            return Ok(Arc::new(Node::Readme));
        }
        Err(if create.is_some() { EROFS } else { ENOENT })
    }

    fn truncate(&self) -> Result<(), isize> {
        Err(EROFS)
    }

    fn read_at(&self, offset: usize, buf: &mut [u8]) -> Result<usize, isize> {
        if matches!(self, Node::Root(_)) {
            return Err(EISDIR);
        }
        let available = CONTENT.len().saturating_sub(offset);
        let n = available.min(buf.len());
        if n != 0 {
            buf[..n].copy_from_slice(&CONTENT[offset..offset + n]);
        }
        Ok(n)
    }

    fn write_at(&self, _offset: usize, _bytes: &[u8], _append: bool) -> Result<usize, isize> {
        Err(EROFS)
    }

    fn read_all(&self) -> Result<Vec<u8>, isize> {
        if matches!(self, Node::Root(_)) {
            return Err(EISDIR);
        }
        Ok(CONTENT.to_vec())
    }

    fn dir_entry(&self, index: usize) -> Result<Option<DirectoryEntry>, isize> {
        if !matches!(self, Node::Root(_)) {
            return Err(ENOTDIR);
        }
        Ok((index == 0).then(|| DirectoryEntry {
            inode_number: FILE_INO,
            name: b"readme".to_vec(),
            file_type: FileType::Regular,
        }))
    }
}
