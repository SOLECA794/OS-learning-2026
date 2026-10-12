//! 最小 ramfs（V2 §9）：根目录扁平持有文件，内容唯一权威在 RAMFS 全局锁内。
//!
//! - 文件名不含前导 '/'；路径按「去前导 '/' 精确匹配」解析（G5 无目录层级）。
//! - boot 时由 `fs::init` 从程序源注册表装载程序文件 + 验收测试文件。
//! - `read_at`/`write_at` 以 OpenFileDesc 的偏移为权威，读写后推进该偏移。

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::AtomicUsize;

use super::file::{FdEntry, OpenFileDesc};
use super::programs::PROGRAMS;
use crate::kernel::task::SpinLock;

const ENOENT: isize = -2;
const EACCES: isize = -13;
const EEXIST: isize = -17;
const EINVAL: isize = -22;

/// ramfs 文件：内容唯一权威（RAMFS 锁内）。
pub struct RamFile {
    pub name: Vec<u8>,
    pub content: Vec<u8>,
}

/// 根文件系统：扁平文件集合。
pub struct RamFs {
    files: Vec<RamFile>,
}

impl RamFs {
    const fn new() -> Self {
        Self { files: Vec::new() }
    }

    fn find(&self, name: &[u8]) -> Option<usize> {
        self.files.iter().position(|f| f.name == name)
    }
}

static RAMFS: SpinLock<RamFs> = SpinLock::new(RamFs::new());

/// boot 装载：程序源（uapp/uapp2/bad）+ 验收测试文件。
pub fn init() {
    let mut fs = RAMFS.lock();
    for p in PROGRAMS {
        fs.files.push(RamFile {
            name: p.name.as_bytes().to_vec(),
            content: p.elf.to_vec(),
        });
    }
    fs.files.push(RamFile {
        name: b"hello.txt".to_vec(),
        content: b"hello ramfs".to_vec(),
    });
    fs.files.push(RamFile {
        name: b"dup.txt".to_vec(),
        content: b"abcdef".to_vec(),
    });
    fs.files.push(RamFile {
        name: b"fork.txt".to_vec(),
        content: b"abcdef".to_vec(),
    });
    fs.files.push(RamFile {
        name: b"cmds.txt".to_vec(),
        content: b"ls\ncat /hello.txt\nrun /uapp2\ncat /nonexist\nrun /nonexist\n".to_vec(),
    });
    drop(fs);
    crate::log_info!("[fs] ramfs init: {} files", {
        let fs = RAMFS.lock();
        fs.files.len()
    });
}

/// 按路径读整个文件内容（exec 用）。去前导 '/' 精确匹配；找不到 → ENOENT。
pub fn read_file_by_path(path: &str) -> Result<Vec<u8>, isize> {
    let name = path.trim_start_matches('/').as_bytes();
    let fs = RAMFS.lock();
    fs.find(name)
        .map(|i| fs.files[i].content.clone())
        .ok_or(ENOENT)
}

/// open：查文件；不存在且 O_CREAT 时创建；O_TRUNC 清空内容。返回打开描述。
pub fn open(name: Vec<u8>, flags: usize) -> Result<FdEntry, isize> {
    // 与 read_file_by_path 一致：去前导 '/'（G5 F1 实机发现 open 缺 trim）。
    let name = name
        .iter()
        .copied()
        .skip_while(|&b| b == b'/')
        .collect::<Vec<u8>>();
    // 根目录（"/"）：返回目录表项（G5 shell ls 用）。
    if name.is_empty() {
        return Ok(FdEntry::Dir(Arc::new(AtomicUsize::new(0))));
    }
    const O_WRONLY: usize = 1;
    const O_RDWR: usize = 2;
    const O_CREAT: usize = 0x40;
    const O_TRUNC: usize = 0x200;
    const O_APPEND: usize = 0x400;

    let accmode = flags & 3; // O_RDONLY=0 / O_WRONLY=1 / O_RDWR=2
    let readable = accmode != O_WRONLY;
    let writable = accmode != 0;
    // O_APPEND：G5 不支持（未实现标志按 Linux 语义忽略或校验——最小集校验非法位）。
    let supported = accmode <= O_RDWR && (flags & !(3 | O_CREAT | O_TRUNC | O_APPEND)) == 0;
    if !supported {
        return Err(EINVAL);
    }

    let mut fs = RAMFS.lock();
    match fs.find(&name) {
        Some(i) => {
            if flags & O_TRUNC != 0 && writable {
                fs.files[i].content.clear();
            }
            // 文件存在 + O_CREAT：Linux 允许（不报 EEXIST）。
        }
        None => {
            if flags & O_CREAT == 0 {
                return Err(ENOENT);
            }
            fs.files.push(RamFile {
                name: name.clone(),
                content: Vec::new(),
            });
        }
    }
    drop(fs);
    Ok(FdEntry::File(Arc::new(OpenFileDesc::new(
        name, readable, writable,
    ))))
}

/// 读：从 desc 偏移复制到用户缓冲（段校验由调用方做），推进共享偏移。
/// 越界/EOF 返回实际可读字节数（可为 0）。
pub fn read_at(desc: &OpenFileDesc, buf: &mut [u8]) -> Result<usize, isize> {
    if !desc.readable {
        return Err(EACCES);
    }
    let mut fs = RAMFS.lock();
    let i = fs.find(&desc.name).ok_or(ENOENT)?;
    let offset = desc.offset.load(core::sync::atomic::Ordering::Relaxed);
    let available = fs.files[i].content.len().saturating_sub(offset);
    let n = core::cmp::min(available, buf.len());
    buf[..n].copy_from_slice(&fs.files[i].content[offset..offset + n]);
    desc.offset
        .store(offset + n, core::sync::atomic::Ordering::Relaxed);
    Ok(n)
}

/// 写：在 desc 偏移处写入数据（越界扩展内容），推进共享偏移。
pub fn write_at(desc: &OpenFileDesc, data: &[u8]) -> Result<usize, isize> {
    if !desc.writable {
        return Err(EACCES);
    }
    let mut fs = RAMFS.lock();
    let i = fs.find(&desc.name).ok_or(ENOENT)?;
    let offset = desc.offset.load(core::sync::atomic::Ordering::Relaxed);
    let end = offset.checked_add(data.len()).ok_or(EINVAL)?;
    let file = &mut fs.files[i];
    if end > file.content.len() {
        file.content.resize(end, 0);
    }
    file.content[offset..end].copy_from_slice(data);
    desc.offset
        .store(end, core::sync::atomic::Ordering::Relaxed);
    Ok(data.len())
}

/// 目录列表（shell ls 用）：返回各文件名字节。
pub fn list_names() -> Vec<Vec<u8>> {
    let fs = RAMFS.lock();
    fs.files.iter().map(|f| f.name.clone()).collect()
}

/// 检查路径是否是已存在文件（shell/exec 判 EISDIR/ENOENT 前可复用）。
pub fn exists(path: &str) -> bool {
    let name = path.trim_start_matches('/').as_bytes();
    let fs = RAMFS.lock();
    fs.find(name).is_some()
}
