# 04 · 文件系统设计

## 1. 目标

自研 VFS 层 + 先 tmpfs/ramfs 后 ext4 的具体文件系统，提供 Linux 语义的 open/read/write/stat/mmap 文件接口与页缓存，支撑 CAgent 文件类测例（一等奖工程量大头在此）与 BuildStorm 的编译 I/O。

当前实现（2026-10-15）：`src/kernel/fs/vfs/` 是路径解析、固定挂载表与共享打开描述入口，
`src/kernel/fs/ramfs/` 持有可写文件树，`staticfs.rs` 提供 `/static/readme` 只读后端；
任务的 FD 槽位仍在 `src/kernel/task/fd_table.rs`。第二后端已在 RISC-V 用户态
`test-vfs` 中验收，在 LoongArch QEMU 中仅由共同启动阶段的内核自检验收；
LoongArch 用户态文件 syscall 尚未落地。
已支持用户态 mkdirat、子目录文件 IO、目录枚举与按子目录路径 exec；详见
`log/rfc/M25-vfs/index.md` 的运行验收。下列四 trait、挂载、ext4 和缓存仍为后续设计，
当前 `VfsNode` 是最小内部节点接口；完整四 trait、通用挂载 syscall、ext4 和页缓存仍为后续设计。
M3 已将 RISC-V `openat` 改为 `(dirfd, path, flags, mode)`，相对路径可从已打开的
目录节点逐段查找；`mkdirat` 也共用目录 fd 路径起点。`AT_FDCWD` 当前指向根目录，
尚无真正的进程 cwd 或权限模型。

## 2. 组成

```
src/kernel/fs/
├── vfs/           VFS 抽象：Inode / Dentry / File / Superblock / 挂载表
├── inode/         文件对象语义：FileOps / InodeOps / DentryOps
├── pagecache/     统一页缓存（inode-local，LRU 回收）
├── tmpfs/         内存文件系统（起点：rootfs 与 /tmp）
├── ramfs/         （如与 tmpfs 合并则省略）
├── ext4/          自研 ext4（起点只读，后读写 + 元数据缓存）
├── devfs/ / procfs/ / sysfs/ / memfs/   伪文件系统
└── notify/        inotify/fanotify（可选，兼容加分）
```

## 3. 核心抽象

- **VFS 四 trait**：`FileSystemOps`（挂载/卸载/超级块）、`SuperBlockOps`（inode 分配）、`InodeOps`（lookup/create/读写入口）、`FileOps`（read/write/mmap/poll/ioctl）。外部语义对齐 Linux，内部实现自研。
- **Dentry 缓存**：路径查找缓存（dentry cache），支持 LRU 与负缓存；一等奖普遍实现并优化。
- **页缓存**：inode-local page cache，LRU 回收、dirty 写回；与块设备驱动打通 flush 路径。
- **挂载**：rootfs 从启动参数挂载；支持挂载表与 bind/loop（按需）。

## 4. 关键语义（对齐 Linux）

| 操作 | 语义要求 |
|---|---|
| open | 标志组合校验（O_*）、权限检查、创建语义、errno 映射 |
| read/write | 偏移语义、部分读写、EBADF/EINVAL 等错误、阻塞/非阻塞 |
| stat | 结构体布局与字段语义对齐 Linux（含 statx） |
| mmap 文件 | MAP_SHARED 写回、MAP_PRIVATE CoW |
| 目录 | readdir/getdents、d_type、挂载点遍历 |
| 链接/权限 | symlink/hardlink、chmod/chown、ACL 不做首期 |
| 伪 fs | procfs 的 /proc/self、sysfs 基础节点（兼容测例需要） |

## 5. 性能关注点（BuildStorm 相关）

- 编译负载 = 大量小文件 open/read/close + 元数据操作：dentry/inode 缓存命中率是首要优化点。
- 页缓存路径：避免每次读都下探块设备；写回合并与 flush 路径打通（一等奖：virtio-blk 写回页缓存为显著提速点）。
- 元数据小缓存：ext4 的 inode/bitmap 缓存（一等奖实践）。
- 锁竞争：同挂载点并行只读访问要避免全局锁串行化（一等奖：lwext4 多核并行改造）。

## 6. 落地顺序（与依赖关系）

1. tmpfs/ramfs：VFS 骨架 + rootfs 可用（M1-M2）。
2. 伪文件系统：devfs/procfs 基础节点（M2）。
3. ext4 只读 → 读写 + 页缓存 + 写回（M2-M3，工程量大头）。
4. dentry/page cache 优化（M3 性能阶段）。

## 7. 验收标准

1. open/read/write/close/stat/readdir 基础测例通过（含错误路径）。
2. tmpfs 挂载为 rootfs，init 可启动并运行测试程序。
3. ext4 读写：创建/删除/大文件/目录遍历/持久化（重启后数据仍在）。
4. mmap 文件 MAP_SHARED 写回正确。
5. 页缓存无泄漏、无脏数据丢失（回归 + 压力测试）。
6. 基准：小文件 open/read 循环、大文件顺序读、stat 延迟纳入性能台账（一等奖用 statx 对比脚本）。
