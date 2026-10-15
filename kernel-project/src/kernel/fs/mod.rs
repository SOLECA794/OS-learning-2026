//! 内核文件系统入口：VFS 拥有路径与打开描述，ramfs/静态树是后端。

pub(crate) mod ramfs;
pub(crate) mod staticfs;
mod vfs;

pub(crate) use vfs::{
    mkdir_at, open_at, read_file, FileType, OpenFileDesc, OpenOptions, SeekWhence, MAX_PATH_LEN,
};

/// 初始化后端并发布唯一的 VFS 根节点。
pub(crate) fn init() {
    vfs::init();
    // 双架构共同执行：两个后端必须经同一条 VFS 路径可读。
    assert_eq!(
        read_file(b"/hello.txt").expect("ramfs read"),
        b"hello ramfs"
    );
    assert_eq!(
        read_file(b"/static/readme").expect("staticfs read"),
        b"staticfs via vfs\n"
    );
    crate::log_info!("VFS_BACKENDS_PASS");
}
