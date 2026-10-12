//! FD 相关系统调用（RV，V2 §9：open/dup/close/read/write 合同）。
//!
//! 路径与缓冲均经 `copy_from_user` 段边界校验（非法指针 → EFAULT，受控）；
//! 非法 fd → EBADF；读写权限与偏移唯一权威在 `OpenFileDesc`（ramfs 内容权威）。

use alloc::sync::Arc;

use super::copy_cstring_from_user;
use crate::kernel::fs::file::{FdEntry, FdTable, OpenFileDesc};
use crate::kernel::fs::ramfs;
use crate::kernel::task;

const EBADF: isize = -9;
const EFAULT: isize = -14;

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
    let desc = match ramfs::open(path, flags) {
        Ok(d) => d,
        Err(e) => return e,
    };
    // O_APPEND 未支持但被 open 校验拒绝；此处只需分发（flags 校验已在 ramfs::open）。
    let _ = O_WRONLY;
    let _ = O_RDWR;
    task::with_current_fds(|fds: &mut FdTable| fds.alloc(FdEntry::File(desc)))
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
    let desc: Option<Arc<OpenFileDesc>> = task::with_current_fds(|fds: &mut FdTable| {
        match fds.get(fd) {
            Some(FdEntry::File(d)) => Some(d.clone()),
            Some(FdEntry::Console) => return None, // fd 0：EOF（控制台输入 G5 未实现）
            None => return None,
        }
    });
    let desc = match desc {
        Some(d) => d,
        None => {
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
    let desc: Option<Arc<OpenFileDesc>> =
        task::with_current_fds(|fds: &mut FdTable| match fds.get(fd) {
            Some(FdEntry::File(d)) => Some(d.clone()),
            Some(FdEntry::Console) => return None,
            None => return None,
        });
    match desc {
        Some(d) => ramfs::write_at(&d, data)
            .map(|_| len as isize)
            .unwrap_or_else(|e| e),
        None => {
            let exists = task::with_current_fds(|fds: &mut FdTable| fds.get(fd).is_some());
            if !exists {
                return EBADF;
            }
            if !data.is_empty() {
                crate::arch::console::kernel_arch_console_write(data.as_ptr(), data.len());
            }
            len as isize
        }
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
