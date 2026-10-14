# M3 `lseek` 文件与目录偏移

- 状态：Closed；日期：2026-10-14。
- 作者：Codex；评审：用户连续要求推进 M3 内核实现；本次限制在新 syscall 的窄范围。

## 目标与依据

现有 `OpenFileDesc` 已独占文件/目录偏移，read/write/getdents 使用相同锁，但缺少移动偏移的系统调用。引入 RISC-V Linux `lseek=62`，参数为 `fd, offset, whence`，返回新偏移。

## 边界与设计

详见 [Implementation Boundary](../../boundary/M3-lseek.md)。仅 Introduce 新系统调用，不替换现有 ABI。`SEEK_SET/CUR/END` 以有符号 `off_t` 计算；普通文件末尾为字节长度，目录末尾为当前条目数，目录 cookie 仍是现有 getdents 索引。目标偏移须在 `0..=isize::MAX`，失败不更新偏移。控制台不可 seek。

只在 VFS 的打开描述持锁完成读取当前偏移、获取 inode 长度与提交新值，不从 syscall 直接访问内部表示。未引入临时桥。

## 验收计划

在 `uapp-vfs` 补正确与错误路径断言；比赛镜像中执行 `make ARCH=riscv64 test-vfs`，双架构构建和格式检查。运行证据后再更新当前契约并收口。

## 收口记录

- `agent-run`：比赛镜像 `zhouzhouyi/os-contest:20260510` 中 `make ARCH=riscv64 test-vfs RUN_ID=20261014-m3-lseek-final` PASS；串口含 `M3_LSEEK_PASS`、目录重读后的 `V4 directory cursor and records PASS`、`VFS_PASS`，证据在 `artifacts/runs/vfs-20261014-m3-lseek-final/`。
- `agent-run`：`make ARCH=riscv64 test-process RUN_ID=20261014-m3-lseek` PASS；`make fmt-check`、`git diff --check` PASS。
- `agent-run`：`make ARCH=riscv64 test-shell RUN_ID=20261014-m3-lseek` PASS；旧 shell 文件操作未回退。
- `agent-run`：比赛镜像隔离副本依次 `make clean`、`make ARCH=loongarch64 defconfig`、`make ARCH=loongarch64 build` PASS；只证明 LoongArch 内核可编译，未运行其用户态。
- 摩擦扫描：偏移只有 `OpenFileDesc::offset` 一份真相；syscall 不持有 inode/偏移锁；失败在写锁内提交前返回。目录长度读取不复制目录项。剩余兼容缺口为 `SEEK_DATA`/`SEEK_HOLE`、真实文件系统目录 cookie 和 LoongArch 用户态。
