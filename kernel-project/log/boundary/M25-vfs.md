# Implementation Boundary：M2.5 最小 VFS

## 基本信息

- 任务：将当前扁平 ramfs/FD 路径收敛为可扩展的 VFS 文件与目录模型。
- 分类：RFC（文件系统 owner、打开文件描述与路径语义的共享契约变更）。
- 日期：2026-10-14。

## 边界

- **目标**：根目录与普通文件有稳定身份；`openat`、`read`、`write`、`getdents64`、`execve` 通过统一的 VFS 对象访问 ramfs，支持至少一层子目录与绝对路径；现有 `/hello.txt` 和 `hello.txt` 等根文件路径保持可用。
- **非目标**：ext4、挂载、symlink、权限/uid、设备文件、cwd/dirfd 相对路径语义、并发多核文件系统、Linux VFS 全量 API。
- **Owner**：`src/fs/` 拥有路径解析、节点身份及具体 ramfs；`kernel::syscall` 拥有用户 ABI/errno/用户指针；`kernel::task` 拥有进程 FD 表与 fork 生命周期。
- **Handoff**：syscall 将已验证的路径/缓冲交给 VFS；VFS 返回可共享的打开描述；FD 表只持有该描述并在 dup/fork 时 clone。
- **Failure**：路径缺失返回 ENOENT，打开目录做普通读写返回 EISDIR；创建/截断失败不得发布半成品。现有 shell/exec 行为回归不通过时不切换 current contract。
- **Cleanup**：最后一个 FD 引用释放打开描述；文件节点内容由 ramfs 持有，目录枚举游标由打开描述持有。

## 受保护契约

- public API：`arch` 入口、`task::with_current_fds` 与用户态 syscall 编号不变。
- ABI / visible semantics：现有 `openat(path, flags, mode)` 私有约定与旧成功路径保持；新增标准参数的 mkdirat；修正非目录 getdents64 为 ENOTDIR，O_APPEND 按实际 EOF 追加；错误用例固定这些行为。
- current contract：分层 ramfs 与统一 VFS 已接线，根文件旧路径保留；用户可创建目录并在子目录读写/枚举/exec，脚本 shell 和控制台读写保持可用。
- correctness invariant：文件内容与偏移各有唯一 owner；dup/fork 共享偏移；失败的 exec 不替换旧地址空间；用户指针只能由 syscall 层校验与拷贝。

## 验收与验证

- **验收标准**：根文件旧用例通过；子目录内创建/读取/枚举/exec 路径通过；重复打开有独立偏移、dup/fork 共享偏移；错误路径与目录读写返回预期 errno。
- **验证方式**：比赛镜像内 `make ARCH=riscv64 check`、定向用户态 QEMU 测试、`make ARCH=riscv64 test-shell`、`make ARCH=riscv64 run-cow`；LoongArch 在隔离副本中 `make ARCH=loongarch64 check`。
- **验证者**：agent-run；比赛镜像中 `make ARCH=riscv64 test-vfs RUN_ID=20261014-vfs-boundary`、`test-process`、`test-shell`、`run-cow`、`test-console` 均通过；完整命令与日志见 RFC 验收表。
- **停止条件**：目标或 ABI 需超出上述范围、需要引入第三方 crate、或评审未接受 RFC。

## 路线

- 设计与评审：见 `log/rfc/M25-vfs/index.md`；用户要求继续推进后完成接线与运行验证。
