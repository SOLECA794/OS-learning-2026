//! VFS：统一路径解析与打开对象，ramfs 是当前根后端。
//!
//! syscall 只传入内核拥有的路径/缓冲；本层不访问 Task 和用户指针。
//! 无 cwd/用户态挂载/符号链接；相对路径可从根或目录 fd 起点解析。

use crate::klib::sync::SpinLock;
use alloc::sync::Arc;
use alloc::vec::Vec;

mod file;
mod mount;
pub(in crate::kernel::fs) mod node;
use super::{ramfs, staticfs};
pub(crate) use file::OpenFileDesc;
use node::NodeRef;

/// 与用户 ABI 数值无关的节点类型。
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileType {
    Regular,
    Directory,
}

/// syscall 已完成标志校验后提交的领域选项，不在 VFS 中解释 Linux O_* 位。
#[derive(Clone, Copy)]
pub(crate) struct OpenOptions {
    pub read: bool,
    pub write: bool,
    pub create: bool,
    pub exclusive: bool,
    pub truncate: bool,
    pub append: bool,
    pub directory: bool,
}

/// 已在 syscall 边界解析的 seek 起点，与 Linux whence 数值解耦。
pub(crate) enum SeekWhence {
    Start,
    Current,
    End,
}

/// 目录项的独立快照，不向 syscall 暴露 inode 锁或目录内部容器。
pub(crate) struct DirectoryEntry {
    pub inode_number: usize,
    pub name: Vec<u8>,
    pub file_type: FileType,
}

/// 由 ramfs 单次锁内读取的节点元数据；不包含 Linux stat 布局或权限位。
pub(crate) struct FileMetadata {
    pub inode_number: usize,
    pub file_type: FileType,
    pub size: usize,
}

pub(in crate::kernel::fs) const ENOENT: isize = -2;
pub(in crate::kernel::fs) const EACCES: isize = -13;
pub(in crate::kernel::fs) const ENOMEM: isize = -12;
pub(in crate::kernel::fs) const EEXIST: isize = -17;
pub(in crate::kernel::fs) const ENOTDIR: isize = -20;
pub(in crate::kernel::fs) const EISDIR: isize = -21;
pub(in crate::kernel::fs) const EINVAL: isize = -22;
pub(in crate::kernel::fs) const ENAMETOOLONG: isize = -36;

pub(crate) const MAX_PATH_LEN: usize = 128;

/// 根节点唯一发布位置；初始化完成后只获取 Arc，不复制树或文件内容。
static ROOT: SpinLock<Option<NodeRef>> = SpinLock::new(None);

/// 引导阶段构造文件树，全部初始化完成后一次性发布。
pub(crate) fn init() {
    let root = ramfs::Inode::new(FileType::Directory);
    for program in ramfs::programs::PROGRAMS {
        let inode = root
            .child(program.name.as_bytes(), Some(FileType::Regular), true)
            .expect("boot program");
        inode
            .write_at(0, program.elf, false)
            .expect("boot ELF content");
    }
    let fixtures: &[(&[u8], &[u8])] = &[
        (b"hello.txt", b"hello ramfs"),
        (b"dup.txt", b"abcdef"),
        (b"fork.txt", b"abcdef"),
        (
            b"cmds.txt",
            b"ls\ncat /hello.txt\nrun /uapp2\ncat /nonexist\nrun /nonexist\n",
        ),
    ];
    for &(name, bytes) in fixtures {
        root.child(name, Some(FileType::Regular), true)
            .expect("boot fixture")
            .write_at(0, bytes, false)
            .expect("boot fixture content");
    }
    // 固定只读挂载点：目录项在 ramfs 中可见，路径穿越时切至另一后端。
    let mountpoint = root
        .child(b"static", Some(FileType::Directory), true)
        .expect("static mountpoint");
    let static_root: NodeRef = staticfs::root(mountpoint.metadata().inode_number);
    let root_node: NodeRef = root;
    mount::register(root_node.clone(), b"static", static_root);
    let mut slot = ROOT.lock();
    assert!(slot.is_none(), "VFS initialized twice");
    *slot = Some(root_node);
    drop(slot);
    crate::log_info!("[fs] VFS ramfs root ready");
}

fn root() -> NodeRef {
    ROOT.lock().as_ref().expect("VFS used before init").clone()
}

/// 根目录的 `static` 项是固定只读挂载点；目录 fd 进入后保存后端根节点。
fn child(
    parent: &NodeRef,
    name: &[u8],
    create: Option<FileType>,
    exclusive: bool,
) -> Result<NodeRef, isize> {
    if let Some(mounted) = mount::resolve(parent, name) {
        if create.is_some() && exclusive {
            return Err(EEXIST);
        }
        return Ok(mounted);
    }
    parent.child(name, create, exclusive)
}

/// 有界逐段解析。重复斜杠合并；末尾斜杠由 open/lookup 检查目录类型。
/// 暂不解析 . 和 ..：显式拒绝，避免把它们创建为普通文件名。
fn components(path: &[u8]) -> Result<Vec<&[u8]>, isize> {
    if path.is_empty() {
        return Err(ENOENT);
    }
    if path.len() >= MAX_PATH_LEN {
        return Err(ENAMETOOLONG);
    }
    if path.contains(&0) {
        return Err(EINVAL);
    }
    let parts: Vec<_> = path
        .split(|&byte| byte == b'/')
        .filter(|part| !part.is_empty())
        .collect();
    if parts.iter().any(|part| *part == b"." || *part == b"..") {
        return Err(EINVAL);
    }
    Ok(parts)
}

/// 逐段克隆节点句柄后释放父目录锁，不携带锁跨越整条路径。
fn walk_from(mut inode: NodeRef, parts: &[&[u8]]) -> Result<NodeRef, isize> {
    for name in parts {
        inode = child(&inode, name, None, false)?;
    }
    Ok(inode)
}

fn walk(parts: &[&[u8]]) -> Result<NodeRef, isize> {
    walk_from(root(), parts)
}

/// 绝对路径与临时根 cwd 使用根节点；相对目录 fd 必须确实指向目录。
fn path_start(path: &[u8], dir: Option<&OpenFileDesc>) -> Result<NodeRef, isize> {
    if path.starts_with(b"/") {
        Ok(root())
    } else if let Some(dir) = dir {
        dir.directory_inode()
    } else {
        Ok(root())
    }
}

/// 绝对路径从根开始；相对路径从传入目录句柄或当前临时根 cwd 开始。
pub(crate) fn open_at(
    path: &[u8],
    options: OpenOptions,
    dir: Option<&OpenFileDesc>,
) -> Result<Arc<OpenFileDesc>, isize> {
    let parts = components(path)?;
    let needs_dir = path.ends_with(b"/") || options.directory;
    let start = path_start(path, dir)?;
    let inode = if let Some((name, parents)) = parts.split_last() {
        let parent = walk_from(start, parents)?;
        let create = if options.create && !needs_dir {
            Some(FileType::Regular)
        } else {
            None
        };
        child(&parent, name, create, options.create && options.exclusive)?
    } else {
        if options.create {
            return Err(EISDIR);
        }
        start
    };
    let is_dir = inode.kind() == FileType::Directory;
    if needs_dir && !is_dir {
        return Err(ENOTDIR);
    }
    if is_dir && (options.write || options.create || options.truncate) {
        return Err(EISDIR);
    }
    if options.truncate && options.write {
        inode.truncate()?;
    }
    Ok(Arc::new(OpenFileDesc::new(inode, options)))
}

/// 父目录必须已存在；不递归创建。目录项发布由 ramfs 目录锁保证原子性。
pub(crate) fn mkdir_at(path: &[u8], dir: Option<&OpenFileDesc>) -> Result<(), isize> {
    let parts = components(path)?;
    let (name, parents) = parts.split_last().ok_or(EEXIST)?;
    let parent = walk_from(path_start(path, dir)?, parents)?;
    child(&parent, name, Some(FileType::Directory), true)?;
    Ok(())
}

/// exec 与 open 共用同一棵树和路径规则；返回独立文件快照。
pub(crate) fn read_file(path: &[u8]) -> Result<Vec<u8>, isize> {
    let inode = walk(&components(path)?)?;
    if path.ends_with(b"/") && inode.kind() != FileType::Directory {
        return Err(ENOTDIR);
    }
    inode.read_all()
}
