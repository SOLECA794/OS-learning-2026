# M3：mkdirat 目录 fd 相对路径

- 日期：2026-10-14
- 分类：Patch；延伸现有 `mkdirat` ABI，不改变系统调用号或参数布局。
- 边界：[M3-mkdirat-dirfd](../boundary/M3-mkdirat-dirfd.md)
- 验证者：agent-run

## 改动

`openat` 与 `mkdirat` 共用 syscall 层的目录 fd 提取与 VFS 层的路径起点选择。绝对路径忽略 `dirfd`；`AT_FDCWD` 暂指向 ramfs 根目录；其他相对路径从已打开的目录描述逐段解析。坏 fd 返回 `EBADF`，非目录 fd 返回 `ENOTDIR`。原有路径规则和 `mode` 暂不实施的边界不变。

## 验证

| 命令 | 结果 |
|---|---|
| 比赛镜像中 `make fmt && make ARCH=riscv64 test-vfs RUN_ID=20261014-m3-mkdirat` | PASS；`artifacts/runs/vfs-20261014-m3-mkdirat/serial.log` 包含 `M3_MKDIRAT_PASS`、`M3_OPENAT_PASS` 与 `VFS_PASS` |
| 比赛镜像隔离副本中 `make clean && make ARCH=loongarch64 defconfig && make ARCH=loongarch64 build` | PASS；只证明 LoongArch 内核可编译，不代表用户态运行通过 |
| `git diff --check` | PASS |

## 未覆盖

真实进程 cwd、权限与 umask、`.`/`..`、符号链接仍未实现；LoongArch 用户态未运行。
