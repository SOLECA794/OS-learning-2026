//! ramfs 节点：目录持有子节点，普通文件持有内容；节点身份不依赖路径名字。
//!
//! 只有 VFS 访问本模块。目录查找/创建在同一把目录锁内完成；文件内容更新
//! 在文件锁内完成。无父节点强引用，整个树没有 Arc 引用环。

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, Ordering};

use super::{
    DirectoryEntry, FileMetadata, FileType, EEXIST, EINVAL, EISDIR, ENOENT, ENOMEM, ENOTDIR,
};
use crate::klib::sync::SpinLock;

/// inode 编号用于目录枚举身份；创建后固定，和路径、打开偏移均无关。
static NEXT_INO: AtomicUsize = AtomicUsize::new(1);

struct DirEntry {
    name: Vec<u8>,
    inode: Arc<Inode>,
}

enum NodeData {
    File(Vec<u8>),
    Directory(Vec<DirEntry>),
}

/// 不透明 ramfs 节点句柄；内容及目录项只在 data 锁内访问。
pub(super) struct Inode {
    ino: usize,
    data: SpinLock<NodeData>,
}

impl Inode {
    pub(super) fn new(kind: FileType) -> Arc<Self> {
        Arc::new(Self {
            ino: NEXT_INO.fetch_add(1, Ordering::Relaxed),
            data: SpinLock::new(match kind {
                FileType::Regular => NodeData::File(Vec::new()),
                FileType::Directory => NodeData::Directory(Vec::new()),
            }),
        })
    }

    pub(super) fn kind(&self) -> FileType {
        match &*self.data.lock() {
            NodeData::File(_) => FileType::Regular,
            NodeData::Directory(_) => FileType::Directory,
        }
    }

    /// 普通文件长度按字节计；目录长度是当前 getdents 游标的条目数。
    pub(super) fn seek_len(&self) -> usize {
        match &*self.data.lock() {
            NodeData::File(bytes) => bytes.len(),
            NodeData::Directory(entries) => entries.len(),
        }
    }

    /// 节点编号不变；类型和逻辑文件长度在同一把内容锁内取快照。
    pub(super) fn metadata(&self) -> FileMetadata {
        let data = self.data.lock();
        let (file_type, size) = match &*data {
            NodeData::File(bytes) => (FileType::Regular, bytes.len()),
            NodeData::Directory(_) => (FileType::Directory, 0),
        };
        FileMetadata {
            inode_number: self.ino,
            file_type,
            size,
        }
    }

    /// 查找或原子创建一个子项。exclusive 防止 O_EXCL / mkdir 覆盖旧节点。
    pub(super) fn child(
        &self,
        name: &[u8],
        create: Option<FileType>,
        exclusive: bool,
    ) -> Result<Arc<Inode>, isize> {
        let mut data = self.data.lock();
        let NodeData::Directory(entries) = &mut *data else {
            return Err(ENOTDIR);
        };
        if let Some(entry) = entries.iter().find(|entry| entry.name == name) {
            return if exclusive {
                Err(EEXIST)
            } else {
                Ok(entry.inode.clone())
            };
        }
        let kind = create.ok_or(ENOENT)?;
        // 先准备容器空间，再发布新节点；失败不改变目录可见内容。
        entries.try_reserve(1).map_err(|_| ENOMEM)?;
        let inode = Inode::new(kind);
        entries.push(DirEntry {
            name: name.to_vec(),
            inode: inode.clone(),
        });
        Ok(inode)
    }

    pub(super) fn truncate(&self) -> Result<(), isize> {
        match &mut *self.data.lock() {
            NodeData::File(bytes) => {
                bytes.clear();
                Ok(())
            }
            NodeData::Directory(_) => Err(EISDIR),
        }
    }

    /// 从指定偏移短读。其它打开描述截断文件后，旧偏移超出 EOF 仍返回 0。
    pub(super) fn read_at(&self, offset: usize, buf: &mut [u8]) -> Result<usize, isize> {
        let data = self.data.lock();
        let NodeData::File(bytes) = &*data else {
            return Err(EISDIR);
        };
        if offset >= bytes.len() {
            return Ok(0);
        }
        let n = buf.len().min(bytes.len() - offset);
        buf[..n].copy_from_slice(&bytes[offset..offset + n]);
        Ok(n)
    }

    /// append 在内容锁内选择 EOF，避免不同打开描述覆盖彼此的追加数据。
    /// 返回写后偏移；调用方拥有打开描述的偏移锁。
    pub(super) fn write_at(
        &self,
        offset: usize,
        bytes: &[u8],
        append: bool,
    ) -> Result<usize, isize> {
        let mut data = self.data.lock();
        let NodeData::File(content) = &mut *data else {
            return Err(EISDIR);
        };
        if bytes.is_empty() {
            return Ok(offset);
        }
        let start = if append { content.len() } else { offset };
        let end = start.checked_add(bytes.len()).ok_or(EINVAL)?;
        if end > content.len() {
            content
                .try_reserve(end - content.len())
                .map_err(|_| ENOMEM)?;
            content.resize(end, 0);
        }
        content[start..end].copy_from_slice(bytes);
        Ok(end)
    }

    /// exec 取得内容快照；返回后不持文件锁，ELF 解析不暴露内部 Vec。
    pub(super) fn read_all(&self) -> Result<Vec<u8>, isize> {
        let data = self.data.lock();
        match &*data {
            NodeData::File(bytes) => Ok(bytes.clone()),
            NodeData::Directory(_) => Err(EISDIR),
        }
    }

    /// 只返回目录项元数据；用户 ABI 记录由 syscall 层编码。
    /// 暂无删除/重命名，因此索引在追加目录项后仍稳定。
    pub(super) fn dir_entry(&self, index: usize) -> Result<Option<DirectoryEntry>, isize> {
        let data = self.data.lock();
        let NodeData::Directory(entries) = &*data else {
            return Err(ENOTDIR);
        };
        Ok(entries.get(index).map(|entry| DirectoryEntry {
            inode_number: entry.inode.ino,
            name: entry.name.clone(),
            file_type: entry.inode.kind(),
        }))
    }
}
