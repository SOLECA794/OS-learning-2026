//! VFS 的内部挂载表：以父节点身份和名字选择后端根节点。
//!
//! 当前只有固定 `/static` 挂载；不提供用户态 mount/umount，也不允许卸载。

use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::klib::sync::SpinLock;

use super::node::NodeRef;

struct Mount {
    parent: NodeRef,
    name: Vec<u8>,
    root: NodeRef,
}

/// 启动时一次性注册，运行时只读；打开句柄持有自己的节点引用。
static MOUNTS: SpinLock<Vec<Mount>> = SpinLock::new(Vec::new());

pub(super) fn register(parent: NodeRef, name: &[u8], root: NodeRef) {
    let mut mounts = MOUNTS.lock();
    assert!(
        !mounts
            .iter()
            .any(|item| Arc::ptr_eq(&item.parent, &parent) && item.name == name),
        "duplicate VFS mountpoint"
    );
    mounts.push(Mount {
        parent,
        name: name.to_vec(),
        root,
    });
}

/// 只克隆节点句柄，不把挂载表锁带进后端操作。
pub(super) fn resolve(parent: &NodeRef, name: &[u8]) -> Option<NodeRef> {
    MOUNTS
        .lock()
        .iter()
        .find(|item| Arc::ptr_eq(&item.parent, parent) && item.name == name)
        .map(|item| item.root.clone())
}
