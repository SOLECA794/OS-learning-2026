//! FD 系统调用：校验用户参数，克隆任务的打开描述，再调用 VFS。
//!
//! 不在 task 锁内进行文件 IO 或控制台等待。文件内容/偏移由 VFS 唯一管理，
//! 用户地址写入前先准备目的页，避免 EFAULT 时已经消费文件或目录游标。

use super::copy_cstring_from_user;
use crate::kernel::fs::{self, OpenFileDesc};
use crate::kernel::task::{self, fd_table::FdEntry};
use alloc::sync::Arc;

const EBADF: isize = -9;
const EFAULT: isize = -14;
const ENOTDIR: isize = -20;
const EISDIR: isize = -21;
const EINVAL: isize = -22;
const ESPIPE: isize = -29;
const AT_FDCWD: isize = -100;

/// Linux O_* 在 ABI 边界转换为 VFS 领域选项，未知位在查路径前拒绝。
fn open_options(flags: usize) -> Result<fs::OpenOptions, isize> {
    const O_CREAT: usize = 0x40;
    const O_EXCL: usize = 0x80;
    const O_TRUNC: usize = 0x200;
    const O_APPEND: usize = 0x400;
    const O_DIRECTORY: usize = 0x10000;
    if flags & 3 == 3 || flags & !(3 | O_CREAT | O_EXCL | O_TRUNC | O_APPEND | O_DIRECTORY) != 0 {
        return Err(EINVAL);
    }
    if flags & (O_CREAT | O_DIRECTORY) == (O_CREAT | O_DIRECTORY) {
        return Err(EINVAL);
    }
    Ok(fs::OpenOptions {
        read: flags & 3 != 1,
        write: flags & 3 != 0,
        create: flags & O_CREAT != 0,
        exclusive: flags & O_EXCL != 0,
        truncate: flags & O_TRUNC != 0,
        append: flags & O_APPEND != 0,
        directory: flags & O_DIRECTORY != 0,
    })
}

/// Linux openat(dirfd, path, flags, mode)。当前无权限模型，mode 暂不实施；
/// AT_FDCWD 暂映射到根目录，进程 cwd 落地后改用调用者的 cwd。
pub fn sys_openat(dirfd: usize, path_ptr: usize, flags: usize, _mode: usize) -> isize {
    let result = (|| {
        let path = copy_cstring_from_user(path_ptr, fs::MAX_PATH_LEN)?;
        let options = open_options(flags)?;
        let base = directory_base(dirfd, &path)?;
        let desc = fs::open_at(&path, options, base.as_deref())?;
        task::with_current_fds(|fds| fds.alloc(FdEntry::File(desc)))
    })();
    result.map(|fd| fd as isize).unwrap_or_else(|e| e)
}

/// mkdirat(dirfd, path, mode)：绝对路径忽略 dirfd；相对路径从目录 fd 解析。
/// 尚无权限模型，mode 暂不实施；引入 credentials/umask 时补齐权限语义。
pub fn sys_mkdirat(dirfd: usize, path_ptr: usize, _mode: usize) -> isize {
    let result = (|| {
        let path = copy_cstring_from_user(path_ptr, fs::MAX_PATH_LEN)?;
        let base = directory_base(dirfd, &path)?;
        fs::mkdir_at(&path, base.as_deref())
    })();
    result.map(|_| 0).unwrap_or_else(|e| e)
}

/// 只从 fd 表取得打开描述；具体节点是否为目录由 VFS 统一判断。
fn directory_base(dirfd: usize, path: &[u8]) -> Result<Option<Arc<OpenFileDesc>>, isize> {
    if path.starts_with(b"/") || dirfd as isize == AT_FDCWD {
        return Ok(None);
    }
    match entry(dirfd)? {
        FdEntry::File(desc) => Ok(Some(desc)),
        FdEntry::ConsoleInput | FdEntry::ConsoleOutput => Err(ENOTDIR),
        FdEntry::PipeRead(_) | FdEntry::PipeWrite(_) => Err(ENOTDIR),
    }
}

fn entry(fd: usize) -> Result<FdEntry, isize> {
    task::with_current_fds(|fds| fds.get(fd).cloned()).ok_or(EBADF)
}

pub fn sys_close(fd: usize) -> isize {
    task::with_current_fds(|fds| fds.close(fd))
        .map(|_| 0)
        .unwrap_or_else(|e| e)
}

/// RISC-V asm-generic `struct stat`：128 字节。syscall 层独占此 UAPI 布局。
/// 当前 ramfs 没有权限、属主或时间戳模型，因此只编码真实可得的类型/大小/编号。
pub fn sys_fstat(fd: usize, stat_ptr: usize) -> isize {
    let target = match entry(fd) {
        Ok(target) => target,
        Err(e) => return e,
    };
    const S_IFREG: u32 = 0o100000;
    const S_IFDIR: u32 = 0o040000;
    const S_IFCHR: u32 = 0o020000;
    const S_IFIFO: u32 = 0o010000;
    let (ino, mode, size) = match target {
        FdEntry::File(desc) => {
            let meta = desc.metadata();
            let mode = match meta.file_type {
                fs::FileType::Regular => S_IFREG,
                fs::FileType::Directory => S_IFDIR,
            };
            (meta.inode_number, mode, meta.size)
        }
        FdEntry::ConsoleInput | FdEntry::ConsoleOutput => (0, S_IFCHR, 0),
        FdEntry::PipeRead(_) | FdEntry::PipeWrite(_) => (0, S_IFIFO, 0),
    };
    if size > isize::MAX as usize {
        return -75; // EOVERFLOW：st_size 是有符号 64 位。
    }
    let mut stat = [0u8; 128];
    stat[8..16].copy_from_slice(&(ino as u64).to_ne_bytes());
    stat[16..20].copy_from_slice(&mode.to_ne_bytes());
    stat[20..24].copy_from_slice(&1u32.to_ne_bytes()); // st_nlink：当前无硬链接。
    stat[48..56].copy_from_slice(&(size as i64).to_ne_bytes());
    stat[56..60].copy_from_slice(&4096i32.to_ne_bytes()); // st_blksize：ramfs 页大小。
    let blocks = size / 512 + usize::from(size % 512 != 0);
    stat[64..72].copy_from_slice(&(blocks as i64).to_ne_bytes());
    super::copy_bytes_to_user(stat_ptr, &stat)
        .map(|_| 0)
        .unwrap_or_else(|e| e)
}

/// Linux lseek(fd, offset, whence)；控制台没有文件偏移。
pub fn sys_lseek(fd: usize, offset: i64, whence: usize) -> isize {
    let desc = match entry(fd) {
        Ok(FdEntry::File(desc)) => desc,
        Ok(FdEntry::ConsoleInput | FdEntry::ConsoleOutput) => return ESPIPE,
        Ok(FdEntry::PipeRead(_) | FdEntry::PipeWrite(_)) => return ESPIPE,
        Err(e) => return e,
    };
    let origin = match whence {
        0 => fs::SeekWhence::Start,
        1 => fs::SeekWhence::Current,
        2 => fs::SeekWhence::End,
        _ => return EINVAL,
    };
    desc.seek(offset, origin)
        .map(|n| n as isize)
        .unwrap_or_else(|e| e)
}

/// 目的页先校验并解除 COW；调用期间内核不抢占，没有同一进程的并发 unmap。
fn prepare_output(buf: usize, len: usize) -> Result<(), isize> {
    if len == 0 {
        return Ok(());
    }
    let end = buf.checked_add(len).ok_or(EFAULT)?;
    if buf < super::USER_TEXT_BASE || end > super::USER_STACK_TOP {
        return Err(EFAULT);
    }
    task::with_current_space(|space| space.prepare_user_write(buf, len))
        .ok_or(EFAULT)?
        .map_err(|_| EFAULT)
}

/// 无输入时睡眠；驱动 IRQ 唤醒后重查队列，不将暂时无数据报告为 EOF。
fn read_console(buf: usize, len: usize) -> isize {
    if len == 0 {
        return 0;
    }
    let count = len.min(256);
    if let Err(e) = prepare_output(buf, count) {
        return e;
    }
    let mut bytes = [0u8; 256];
    loop {
        let n = crate::arch::console::read_available(&mut bytes[..count]);
        if n != 0 {
            return super::copy_bytes_to_user(buf, &bytes[..n])
                .map(|_| n as isize)
                .unwrap_or_else(|e| e);
        }
        // LA 尚无串口 RX 中断，空输入返回 EAGAIN，不让任务永远睡眠。
        if !task::block_current_on_console_if_empty() {
            return -11;
        }
    }
}

pub fn sys_read(fd: usize, buf: usize, len: usize) -> isize {
    let desc = match entry(fd) {
        Ok(FdEntry::ConsoleInput) => return read_console(buf, len),
        // 保留现有 stdout/stderr read 返回 EOF 的行为。
        Ok(FdEntry::ConsoleOutput) => return 0,
        Ok(FdEntry::File(desc)) => desc,
        Ok(FdEntry::PipeRead(pipe)) => {
            let count = len.min(crate::kernel::mm::address::PAGE_SIZE);
            if let Err(e) = prepare_output(buf, count) {
                return e;
            }
            let mut bytes = alloc::vec![0u8; count];
            let n = pipe.read(&mut bytes);
            return super::copy_bytes_to_user(buf, &bytes[..n])
                .map(|_| n as isize)
                .unwrap_or_else(|e| e);
        }
        Ok(FdEntry::PipeWrite(_)) => return EISDIR,
        Err(e) => return e,
    };
    if desc.is_dir() {
        return EISDIR;
    }
    read_into_user(desc, buf, len)
}

/// Linux pread64：先准备用户目标页，再读指定文件位置；不消费打开描述游标。
pub fn sys_pread64(fd: usize, buf: usize, len: usize, offset: i64) -> isize {
    let desc = match entry(fd) {
        Ok(FdEntry::File(desc)) => desc,
        Ok(FdEntry::ConsoleInput | FdEntry::ConsoleOutput) => return ESPIPE,
        Ok(FdEntry::PipeRead(_) | FdEntry::PipeWrite(_)) => return ESPIPE,
        Err(e) => return e,
    };
    if desc.is_dir() {
        return EISDIR;
    }
    if offset < 0 {
        return EINVAL;
    }
    let count = len.min(crate::kernel::mm::address::PAGE_SIZE);
    if let Err(e) = prepare_output(buf, count) {
        return e;
    }
    let mut bytes = alloc::vec![0; count];
    let n = match desc.read_at(offset as usize, &mut bytes) {
        Ok(n) => n,
        Err(e) => return e,
    };
    super::copy_bytes_to_user(buf, &bytes[..n])
        .map(|_| n as isize)
        .unwrap_or_else(|e| e)
}

/// 文件读取最多一页，防止用户长度造成无界堆分配。
/// 先准备用户页，再推进打开描述偏移，最后写回已经准备好的目的缓冲。
fn read_into_user(desc: Arc<OpenFileDesc>, buf: usize, len: usize) -> isize {
    let count = len.min(crate::kernel::mm::address::PAGE_SIZE);
    if let Err(e) = prepare_output(buf, count) {
        return e;
    }
    let mut bytes = alloc::vec![0; count];
    let n = match desc.read(&mut bytes) {
        Ok(n) => n,
        Err(e) => return e,
    };
    super::copy_bytes_to_user(buf, &bytes[..n])
        .map(|_| n as isize)
        .unwrap_or_else(|e| e)
}

pub fn sys_write(fd: usize, buf: usize, len: usize) -> isize {
    let target = match entry(fd) {
        Ok(entry) => entry,
        Err(e) => return e,
    };
    let count = len.min(crate::kernel::mm::address::PAGE_SIZE);
    let bytes = match super::copy_from_user(buf, count) {
        Ok(bytes) => bytes,
        Err(e) => return e,
    };
    match target {
        FdEntry::ConsoleInput | FdEntry::ConsoleOutput => {
            if !bytes.is_empty() {
                crate::arch::console::kernel_arch_console_write(bytes.as_ptr(), bytes.len());
            }
            bytes.len() as isize
        }
        FdEntry::File(desc) => desc.write(&bytes).map(|n| n as isize).unwrap_or_else(|e| e),
        FdEntry::PipeWrite(pipe) => pipe.write(&bytes) as isize,
        FdEntry::PipeRead(_) => -EINVAL,
    }
}

/// Linux pwrite64：源缓冲先复制到内核，再写指定位置；共享游标不改变。
pub fn sys_pwrite64(fd: usize, buf: usize, len: usize, offset: i64) -> isize {
    let desc = match entry(fd) {
        Ok(FdEntry::File(desc)) => desc,
        Ok(FdEntry::ConsoleInput | FdEntry::ConsoleOutput) => return ESPIPE,
        Ok(FdEntry::PipeRead(_) | FdEntry::PipeWrite(_)) => return ESPIPE,
        Err(e) => return e,
    };
    if desc.is_dir() {
        return EISDIR;
    }
    if offset < 0 {
        return EINVAL;
    }
    let count = len.min(crate::kernel::mm::address::PAGE_SIZE);
    let bytes = match super::copy_from_user(buf, count) {
        Ok(bytes) => bytes,
        Err(e) => return e,
    };
    desc.write_at(offset as usize, &bytes)
        .map(|n| n as isize)
        .unwrap_or_else(|e| e)
}

pub fn sys_dup(fd: usize) -> isize {
    task::with_current_fds(|fds| {
        let entry = fds.get(fd).cloned().ok_or(EBADF)?;
        fds.alloc(entry)
    })
    .map(|fd| fd as isize)
    .unwrap_or_else(|e| e)
}

/// dup2(oldfd, newfd)：把 oldfd 复制到 newfd；newfd 已有则先关闭。
pub fn sys_dup2(oldfd: usize, newfd: usize) -> isize {
    task::with_current_fds(|fds| {
        let entry = fds.get(oldfd).cloned().ok_or(EBADF)?;
        if oldfd == newfd {
            return Ok(newfd as isize);
        }
        fds.set(newfd, entry)?;
        Ok(newfd as isize)
    })
    .unwrap_or_else(|e| e)
}

/// pipe(fds_ptr)：创建管道，fds[0]=读端，fds[1]=写端。
pub fn sys_pipe(fds_ptr: usize) -> isize {
    use crate::kernel::fs::Pipe;
    let pipe = Pipe::new();
    let result = task::with_current_fds(|fds| {
        let read_fd = fds.alloc(FdEntry::PipeRead(pipe.clone()))?;
        let write_fd = fds.alloc(FdEntry::PipeWrite(pipe.clone()))?;
        Ok([read_fd, write_fd])
    });
    match result {
        Ok([r, w]) => {
            if super::copy_bytes_to_user(fds_ptr, &(r as u32).to_ne_bytes()).is_err() {
                return super::EFAULT;
            }
            if super::copy_bytes_to_user(fds_ptr + 4, &(w as u32).to_ne_bytes()).is_err() {
                return super::EFAULT;
            }
            0
        }
        Err(e) => e,
    }
}

/// 非目录为 ENOTDIR；缓冲放不下一个完整记录为 EINVAL，EOF 为 0。
pub fn sys_getdents64(fd: usize, buf: usize, len: usize) -> isize {
    let desc = match entry(fd) {
        Ok(FdEntry::File(desc)) if desc.is_dir() => desc,
        Ok(_) => return ENOTDIR,
        Err(e) => return e,
    };
    let count = len.min(crate::kernel::mm::address::PAGE_SIZE);
    if let Err(e) = prepare_output(buf, count) {
        return e;
    }
    let mut bytes = alloc::vec![0u8; count];
    let mut written = 0;
    let result = desc.read_dir(|entry, next_position| {
        // linux_dirent64：19 字节头、名字、NUL，然后按 8 字节对齐。
        let reclen = (19 + entry.name.len() + 1 + 7) & !7;
        if reclen > count - written {
            return if written == 0 { Err(EINVAL) } else { Ok(false) };
        }
        let record = &mut bytes[written..written + reclen];
        record[..8].copy_from_slice(&(entry.inode_number as u64).to_ne_bytes());
        record[8..16].copy_from_slice(&(next_position as i64).to_ne_bytes());
        record[16..18].copy_from_slice(&(reclen as u16).to_ne_bytes());
        record[18] = match entry.file_type {
            fs::FileType::Regular => 8,
            fs::FileType::Directory => 4,
        };
        record[19..19 + entry.name.len()].copy_from_slice(&entry.name);
        written += reclen;
        Ok(true)
    });
    if let Err(e) = result {
        return e;
    }
    super::copy_bytes_to_user(buf, &bytes[..written])
        .map(|_| written as isize)
        .unwrap_or_else(|e| e)
}
