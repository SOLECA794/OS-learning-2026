# M3 `pread64` / `pwrite64`

- 状态：Closed；日期：2026-10-14。
- 作者：Codex；评审：用户继续要求推进 M3 内核实现，本次仅引入独立的定位读写 syscall。

## Target 与设计

现有 `OpenFileDesc` 可通过 `read/write` 操作共享偏移；新增 Linux RISC-V 编号 67/68 的定位 IO。ABI 只在 syscall 边界出现，传给 VFS 的偏移为校验后的非负 `usize`。VFS 新增不取偏移锁的定位方法，由 inode 内容锁保证文件字节一致，绝不写共享游标。

RISC-V Linux 对 `O_APPEND` 打开描述的 `pwrite64` 仍追加到文件末尾；沿用 ramfs `write_at(..., append)` 的既有选择，保持此行为。真实存储层、全量 Linux 文件错误矩阵不属于此阶段。

边界详见 [Implementation Boundary](../../boundary/M3-positioned-io.md)。一次切换，不添加兼容桥；先补定向用户态断言，再运行 QEMU 与双架构构建。失败时保留源代码差异与日志供审查，不清理用户工作区。

## 收口记录

- `agent-run`：比赛镜像 `zhouzhouyi/os-contest:20260510` 中 `make ARCH=riscv64 test-vfs RUN_ID=20261014-m3-positioned` PASS；`artifacts/runs/vfs-20261014-m3-positioned/serial.log` 含 `M3_POSITIONED_IO_PASS`、`M3_LSEEK_PASS`、`VFS_PASS`。
- `agent-run`：`make ARCH=riscv64 test-process RUN_ID=20261014-m3-positioned`、`make ARCH=riscv64 test-shell RUN_ID=20261014-m3-positioned`、`make fmt-check`、`git diff --check` 均 PASS。
- `agent-run`：比赛镜像隔离副本中依次 `make clean`、`make ARCH=loongarch64 defconfig`、`make ARCH=loongarch64 build` PASS；未运行 LoongArch 用户态。
- 摩擦扫描：共享偏移仍仅属于 `OpenFileDesc`，定位 IO 只访问 inode 内容锁，无第二份游标或 syscall 层 inode 穿透。输入在写入 inode 前复制，失败不改文件；无新增长期资源。剩余限制为单页 IO 上限、ramfs 无持久化，以及未覆盖完整 Linux 错误矩阵。
