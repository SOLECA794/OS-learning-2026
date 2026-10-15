# RFC：文件系统归入 kernel 与可扩展 VFS

## 状态

- 状态：Closed（本轮目标已完成；LoongArch 用户态文件 ABI 仍属后续阶段）。
- 日期：2026-10-15。
- 作者：Codex。
- 评审：项目维护者（用户在 2026-10-15 明确要求开始编写）。
- 实施边界：`log/boundary/M3-kernel-fs-vfs.md`。

## 1. 目标

把当前顶层 `src/fs` 的内核文件系统代码归入 `src/kernel/fs`，同时把 VFS 对具体 `ramfs::Inode` 的依赖切断，使第二个文件系统至少可通过同一组 `open/read` 路径接入；原有 `getdents/exec` 仍经 VFS 且不退化。目录移动与多后端能力必须分清：仅移动文件不算完成目标。

本轮不实现 ext4、块设备、页缓存、通用 Linux 挂载 ABI、权限、软链接或持久化。第二后端只承担接口验收，不将测试后端包装成生产文件系统。

## 2. 动机与现状证据

- `src/main.rs` 声明顶层 `mod fs`；`kernel::task`、`kernel::syscall` 通过 `crate::fs` 调用它。这些都是内核语义，当前目录层级与期望的 `kernel/fs` 不一致。
- `src/fs/mod.rs` 的 `ROOT` 类型为 `Arc<ramfs::Inode>`，`src/fs/file.rs` 的打开描述也直接持 `Arc<ramfs::Inode>`；路径与文件操作只能作用于 ramfs，当前还不是多后端 VFS。
- `docs/04-filesystem.md` 已明确“第二个后端接入时再确定最小公共 trait”；现在提出多文件系统 VFS，正是确定该接口的时机。
- 已关闭的 `M25-vfs` RFC 将 `src/fs` 定为 owner；本 RFC 只提出后续替换，不改写历史记录。

## 3. 目标目录与所有权

```text
src/kernel/
├── fs/
│   ├── mod.rs             # 初始化与根文件系统装配；只导出窄 VFS 入口
│   ├── vfs/
│   │   ├── mod.rs         # 路径解析、根/挂载解析、领域选项与元数据
│   │   ├── node.rs        # 后端节点操作契约
│   │   ├── file.rs        # 打开描述、访问模式、共享偏移
│   │   └── mount.rs       # 挂载点身份与选择；按实际验收需要落地
│   └── ramfs/
│       ├── mod.rs         # ramfs 节点树与内容操作
│       └── programs.rs    # 启动时嵌入的用户程序源
├── task/fd_table.rs       # 进程 FD 槽位，不持后端内部类型
└── syscall/fd.rs          # Linux ABI 编解码、用户内存校验
```

最终 `kernel::fs::vfs` 拥有路径解析、挂载解析、`OpenFileDesc`、`OpenOptions`、`FileMetadata` 与 `DirectoryEntry`；`kernel::fs::ramfs` 只拥有节点及内容。`kernel::task` 不感知 ramfs，`kernel::syscall` 不感知节点内部锁和后端类型。`exec` 继续经 VFS 读取文件快照。

## 4. 最小跨后端契约

VFS 接受内核拥有的路径字节串，解析组件，依据根/目录 fd/挂载点选出节点。节点句柄是 `Arc<dyn VfsNode>` 或等价的对象安全内部接口；只暴露已由现有调用者需要的操作：

```rust
trait VfsNode: Send + Sync {
    fn metadata(&self) -> FileMetadata;
    fn lookup(&self, name: &[u8]) -> Result<NodeRef, isize>;
    fn create(&self, name: &[u8], kind: FileType, exclusive: bool)
        -> Result<NodeRef, isize>;
    fn truncate(&self) -> Result<(), isize>;
    fn read_at(&self, offset: usize, buf: &mut [u8]) -> Result<usize, isize>;
    fn write_at(&self, offset: usize, bytes: &[u8], append: bool)
        -> Result<usize, isize>;
    fn dir_entry(&self, index: usize) -> Result<Option<DirectoryEntry>, isize>;
}
```

此为**待实现时收窄的示意契约**，不是已发布 Rust API。`lookup` 与 `create` 的分离不得使 `O_EXCL` 或 mkdir 失去目录锁内的原子检查；若无法保持，改用后端原子的 `lookup_or_create`。目录项名字和节点句柄的所有权必须明确；不能让路径缓存成为第二份身份真相。打开偏移仍由 VFS 的 `OpenFileDesc` 唯一持有，后端不接触任务或用户指针。

挂载表只保存挂载点与后端根节点句柄；解析路径时跨挂载点切换节点。第一阶段可仅装配根 ramfs，但验收多后端时须用第二个最小后端实际跨挂载点读取，证明同一路径机制可调度不同后端。挂载/卸载 syscall 不在本轮；测试后端的生命周期和挂载点需在实现前审定。

## 5. 实施与 cutover

1. 在旧路径仍有效时准备节点接口和后端一致性测试，不创建第二份根目录。确认 `O_EXCL`、目录遍历、偏移、`exec` 快照的原有不变量。
2. 一次最终 cutover：迁移文件，更新 `main.rs`、`kernel/mod.rs`、task、syscall、exec 的引用；删除旧 `src/fs` 模块，不保留 `crate::fs` 兼容转发。
3. 按边界运行格式、双架构构建与已授权的 QEMU 文件线回归。所有结果按 `agent-run`、`user-run`、`not-run` 标记；只有通过后才更新架构文档中的 effective owner。

如果只完成目录迁移而节点仍是具体 ramfs 类型，状态只能标为“目录迁移完成，多后端 VFS 未完成”。若需要额外独立 cutover 或改变 ABI，必须修订 RFC 再评审。

## 6. 验收与停止条件

- 静态检查：`rg 'crate::fs|src/fs' src` 不再命中运行时代码；VFS 的节点/打开描述不引用 `ramfs::Inode`。
- 功能检查：ramfs 的原有 `test-vfs`、`test-process`、`test-shell`、`test-console` 无退化；第二个最小后端经 VFS 路径完成打开与读取；双架构构建通过。
- 禁止以 `cargo check` 或单架构 QEMU 输出代替双架构构建/文件线验收。LoongArch 仅构建通过时仍不得声称其用户态文件功能运行通过。
- 若第二后端需求迫使 VFS 公开后端锁、改变共享偏移/错误码、引入静默兼容桥或改变用户 ABI，停止并回到评审。

## 7. 开放问题与已接受限制

- 第二后端采用只读静态文件树还是测试专用设备树？评审需选定，避免仅凭未使用的 trait 宣称多后端支持。
- 具体挂载点如何与根 ramfs 的目录项衔接、是否支持嵌套挂载、卸载时打开句柄如何存活：本轮至少明确一个固定挂载点；其余可列为限制，不伪装成完整 mount。
- `FileMetadata` 的 inode 号在多后端下可能冲突；现有 `fstat` 只报告基础字段。需决定是否引入文件系统身份 (`st_dev`) 并检查 ABI 兼容范围。
- rootfs 仍为易失性 ramfs；没有 ext4 与持久化保证。

## 评审结论

Review 1：维护者明确要求实施。第二后端选固定只读静态树，挂载点 `/static`；
不宣称它是 ext4 或持久化文件系统。

Closure（agent-run，2026-10-15）：代码迁至 `kernel/fs`，VFS 持对象安全 `VfsNode`
句柄及固定挂载表；ramfs 和 staticfs 通过同一路径解析。双架构共享启动阶段
执行 `VFS_BACKENDS_PASS` 自检。LoongArch 首次运行在 ramfs 扩容时触发
compiler-builtins `memset` 内的 LSX 指令（trap code 16）；改用显式逐字节初始化/写入后
复跑通过。这个兼容修复位于 ramfs 写入路径，不代表其它内存原语均已审计。

| 比赛镜像中的命令 | 结果 | 关键标记 / 证据目录 |
|---|---|---|
| `make ARCH=riscv64 test-vfs RUN_ID=20261015-m3-kernel-fs-final` | PASS | `VFS_BACKENDS_PASS`、`VFS_STATIC_PASS`、`VFS_PASS`；`artifacts/runs/vfs-20261015-m3-kernel-fs-final/` |
| `make ARCH=riscv64 test-process RUN_ID=20261015-m3-kernel-fs-final` | PASS | task 0 退出码 7；`artifacts/runs/process-20261015-m3-kernel-fs-final/` |
| `make ARCH=riscv64 test-shell RUN_ID=20261015-m3-kernel-fs-final` | PASS | `shell: done`；`artifacts/runs/shell-20261015-m3-kernel-fs-final/` |
| `make ARCH=riscv64 test-console RUN_ID=20261015-m3-kernel-fs-final` | PASS | `INPUT_PASS`；`artifacts/runs/console-20261015-m3-kernel-fs-final/` |
| 隔离副本 `make ARCH=loongarch64 test RUN_ID=20261015-m3-kernel-fs-la-no-lsx` | PASS | `VFS_BACKENDS_PASS`、M1 启动标记；日志复制至 `artifacts/runs/20261015-m3-kernel-fs-la-no-lsx/` |
| `make fmt-check` | PASS | Rust 格式检查 |

LoongArch 当前没有用户态文件测试入口；该平台的 PASS 仅证明内核初始化、路径解析
和两个后端的内容读取，不证明 `openat` 等 syscall 在 LoongArch 用户态可用。
