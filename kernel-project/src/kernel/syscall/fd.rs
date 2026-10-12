//! FD 相关系统调用（RV，V2 §9：open/dup/close/read/write 合同）。
//!
//! 路径与缓冲均经 `copy_from_user` 段边界校验（非法指针 → EFAULT，受控）；
//! 非法 fd → EBADF；读写权限与偏移唯一权威在 `OpenFileDesc`（ramfs 内容权威）。

use alloc::sync::Arc;
use core::sync::atomic::{AtomicUsize, Ordering};

use super::copy_cstring_from_user;
use crate::kernel::fs::file::{FdEntry, FdTable, OpenFileDesc};
use crate::kernel::fs::ramfs;
use crate::kernel::task;

const EBADF: isize = -9;
const EFAULT: isize = -14;
const EISDIR: isize = -21;

/// openat(2) 最小实现（RV ABI 无 open，用户程序用 openat=56）。
/// 路径按「去前导 '/' 精确匹配」；支持 O_RDONLY/O_WRONLY/O_RDWR/O_CREAT/O_TRUNC。
pub fn sys_openat(path_ptr: usize, flags: usize, _mode: usize) -> isize {
    const O_WRONLY: usize = 1;
    const O_RDWR: usize = 2;
    let path = match copy_cstring_from_user(path_ptr, 128) {
        Ok(p) => p,
        Err(e) => return e,
    };
    if path.is_empty() {
        return -2; // ENOENT（空路径）
    }
    let entry = match ramfs::open(path, flags) {
        Ok(e) => e,
        Err(errno) => return errno,
    };
    // O_APPEND 未支持但被 open 校验拒绝；此处只需分发（flags 校验已在 ramfs::open）。
    let _ = O_WRONLY;
    let _ = O_RDWR;
    task::with_current_fds(|fds: &mut FdTable| fds.alloc(entry))
        .map(|fd| fd as isize)
        .unwrap_or_else(|e| e)
}

/// close(2)：关闭 fd；非法 fd → EBADF。
pub fn sys_close(fd: usize) -> isize {
    task::with_current_fds(|fds: &mut FdTable| fds.close(fd))
        .map(|_| 0)
        .unwrap_or_else(|e| e)
}

/// read(2)：fd=0 控制台返回 EOF(0)；文件按共享偏移读。
pub fn sys_read(fd: usize, buf: usize, len: usize) -> isize {
    // 取出描述（Arc clone），随后在 RAMFS 锁外做内容 IO。
    let desc = task::with_current_fds(
        |fds: &mut FdTable| -> Result<Option<Arc<OpenFileDesc>>, isize> {
            match fds.get(fd) {
                Some(FdEntry::File(d)) => Ok(Some(d.clone())),
                Some(FdEntry::Dir(_)) => Err(EISDIR),
                Some(FdEntry::Console) => Ok(None), // fd 0：EOF（控制台输入 G5 未实现）
                None => Ok(None),
            }
        },
    );
    let desc = match desc {
        Ok(Some(d)) => d,
        Err(e) => return e,
        Ok(None) => {
            // 区分"控制台 EOF(0)"与"非法 fd(EBADF)"：再次查询表项存在性。
            let exists = task::with_current_fds(|fds: &mut FdTable| fds.get(fd).is_some());
            return if exists { 0 } else { EBADF };
        }
    };
    if len == 0 {
        return 0;
    }
    let end = buf.checked_add(len).ok_or(EFAULT).unwrap_or(usize::MAX);
    if buf < super::USER_TEXT_BASE || end > super::USER_STACK_TOP {
        return EFAULT;
    }
    let mut tmp = alloc::vec![0u8; len];
    let n = match ramfs::read_at(&desc, &mut tmp) {
        Ok(n) => n,
        Err(e) => return e,
    };
    if n > 0 {
        // SAFETY: 边界已校验（同 copy_from_user），SUM=1 处理期内核可写用户页。
        unsafe {
            core::ptr::copy_nonoverlapping(tmp.as_ptr(), buf as *mut u8, n);
        }
    }
    n as isize
}

/// write(2)：fd 1/2 控制台；文件按共享偏移写（越界扩展内容）。
pub fn sys_write(fd: usize, buf: usize, len: usize) -> isize {
    let data = match super::copy_from_user(buf, len) {
        Ok(d) => d,
        Err(e) => return e,
    };
    let desc = task::with_current_fds(
        |fds: &mut FdTable| -> Result<Option<Arc<OpenFileDesc>>, isize> {
            match fds.get(fd) {
                Some(FdEntry::File(d)) => Ok(Some(d.clone())),
                Some(FdEntry::Dir(_)) => Err(EISDIR),
                Some(FdEntry::Console) => Ok(None),
                None => Ok(None),
            }
        },
    );
    let desc = match desc {
        Ok(Some(d)) => d,
        Err(e) => return e,
        Ok(None) => {
            let exists = task::with_current_fds(|fds: &mut FdTable| fds.get(fd).is_some());
            if !exists {
                return EBADF;
            }
            if !data.is_empty() {
                crate::arch::console::kernel_arch_console_write(data.as_ptr(), data.len());
            }
            return len as isize;
        }
    };
    match ramfs::write_at(&desc, data) {
        Ok(_) => len as isize,
        Err(e) => e,
    }
}

/// dup(2)：新 fd 指向同一打开文件描述（共享偏移）。
pub fn sys_dup(fd: usize) -> isize {
    task::with_current_fds(|fds: &mut FdTable| {
        let entry = match fds.get(fd) {
            Some(e) => e.clone(),
            None => return EBADF,
        };
        match fds.alloc(entry) {
            Ok(newfd) => newfd as isize,
            Err(e) => e,
        }
    })
}

/// getdents64(2)（SYS=217，G5 shell）：列出打开的目录项（Linux dirent64 布局）。
/// 每次调用从目录游标起填充若干 dirent64，返回写入字节数；耗尽返回 0。
pub fn sys_getdents64(fd: usize, buf: usize, len: usize) -> isize {
    let cursor = task::with_current_fds(
        |fds: &mut FdTable| -> Result<Option<Arc<AtomicUsize>>, isize> {
            match fds.get(fd) {
                Some(FdEntry::Dir(c)) => Ok(Some(c.clone())),
                Some(_) => Ok(None), // 非目录 fd
                None => Err(EBADF),
            }
        },
    );
    let cursor = match cursor {
        Ok(Some(c)) => c,
        Ok(None) => return EISDIR,
        Err(e) => return e,
    };
    let end = buf.checked_add(len).ok_or(EFAULT).unwrap_or(usize::MAX);
    if buf < super::USER_TEXT_BASE || end > super::USER_STACK_TOP {
        return EFAULT;
    }
    let names = ramfs::list_names();
    let start = cursor.load(Ordering::Relaxed);
    if start >= names.len() {
        return 0;
    }
    let mut written = 0usize;
    for (i, name) in names.iter().skip(start).enumerate() {
        let idx = start + i;
        let reclen = ((19 + name.len()) + 7) / 8 * 8;
        if written + reclen > len {
            break;
        }
        let dst = buf + written;
        // SAFETY: 边界已校验（段内），SUM=1 处理期内核可写用户页。
        unsafe {
            core::ptr::write(dst as *mut u64, (idx + 1) as u64); // d_ino
            core::ptr::write((dst + 8) as *mut u64, (idx + 1) as u64); // d_off（下一项序号）
            core::ptr::write((dst + 16) as *mut u16, reclen as u16); // d_reclen
            core::ptr::write((dst + 18) as *mut u8, 8); // d_type = DT_REG
            core::ptr::copy_nonoverlapping(name.as_ptr(), (dst + 19) as *mut u8, name.len());
            core::ptr::write((dst + 19 + name.len()) as *mut u8, 0); // name null 终止
        }
        cursor.store(idx + 1, Ordering::Relaxed);
        written += reclen;
    }
    written as isize
}
