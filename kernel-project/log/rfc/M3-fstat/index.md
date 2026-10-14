# M3 `fstat` 基础元数据

- 状态：Closed；日期：2026-10-14。
- 作者：Codex；评审：用户持续要求推进 M3 内核实现，本次限于新 `fstat` ABI 的基础字段。

## 目标和依据

比赛镜像的 `/usr/riscv64-linux-gnu/include/asm/stat.h` 引用 `asm-generic/stat.h`，`asm-generic/unistd.h` 定义 `__NR_fstat=80`。64 位布局的字段偏移为：`st_ino=8`、`st_mode=16`、`st_size=48`、`st_blksize=56`、`st_blocks=64`，总长度 128 字节。

## 设计

边界见 [Implementation Boundary](../../boundary/M3-fstat.md)。ramfs 返回单次锁内读取的编号、类型与逻辑长度快照；syscall 按固定 ABI 偏移编码，所有保留字段置零。`st_mode` 只填充文件类型位：常规文件、目录、字符设备；权限位为零，不能被解释成已实现权限校验。`st_nlink=1` 是 ramfs 无删除/硬链接阶段的简化值；`st_dev=0`、时间戳与 uid/gid 均为未建模占位。`st_blocks` 按 512 字节逻辑长度取整，不能用来断言实际 RAM 占用。

一次引入，不替换原有 ABI。先通过用户态断言再更新当前契约，失败保留证据供审查。

## 收口记录

- `agent-run`：比赛镜像 `zhouzhouyi/os-contest:20260510` 中 `make ARCH=riscv64 test-vfs RUN_ID=20261014-m3-fstat` PASS；串口 `artifacts/runs/vfs-20261014-m3-fstat/serial.log` 含 `M3_FSTAT_PASS`、`VFS_PASS`。用户态校验 inode、类型、长度、块信息、128 字节 canary、EBADF 和 EFAULT。
- `agent-run`：`make ARCH=riscv64 test-process RUN_ID=20261014-m3-fstat`、`make ARCH=riscv64 test-shell RUN_ID=20261014-m3-fstat`、`make fmt-check`、`git diff --check` 均 PASS。
- `agent-run`：比赛镜像隔离副本依次 `make clean`、`make ARCH=loongarch64 defconfig`、`make ARCH=loongarch64 build` PASS；未运行 LoongArch 用户态。
- 摩擦扫描：inode 编号/类型/长度仍只在 ramfs inode，VFS 向 syscall 给单次快照而非内部锁；Linux 布局仅在 syscall；无第二份状态或长期资源。阶段限制是权限、时间戳、设备号与目录块长度尚未建模，不能声称完整 `fstat` 兼容。
